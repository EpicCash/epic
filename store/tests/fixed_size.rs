// Copyright 2026 The Epic Cash Developers
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use epic_core::ser::{self, ProtocolVersion, Readable, Reader, Writeable, Writer};
use epic_store::types::{DataFile, SizeInfo};

#[derive(Debug, PartialEq)]
struct TestRecord(Vec<u8>);

impl Writeable for TestRecord {
	fn write<W: Writer>(&self, writer: &mut W) -> Result<(), ser::Error> {
		for byte in &self.0 {
			writer.write_u8(*byte)?;
		}
		Ok(())
	}
}

impl Readable for TestRecord {
	fn read(reader: &mut dyn Reader) -> Result<Self, ser::Error> {
		Ok(Self(reader.read_fixed_bytes(2)?))
	}
}

#[test]
fn fixed_size_append_rejects_wrong_width_without_changing_buffer() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("fixed.bin");
	let mut file = DataFile::<TestRecord>::open(
		&path,
		SizeInfo::FixedSize(2),
		ProtocolVersion::local(),
	)
	.unwrap();

	let err = file.append(&TestRecord(vec![1])).unwrap_err();
	assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);

	// The next valid record must start at byte zero, proving the failed append
	// did not leave a partial fixed-width record in the pending buffer.
	assert_eq!(file.append(&TestRecord(vec![2, 3])).unwrap(), 1);
	file.flush().unwrap();
	assert_eq!(std::fs::read(&path).unwrap(), vec![2, 3]);
	assert_eq!(file.read(1), Some(TestRecord(vec![2, 3])));
}
