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

//! Proof deserialization

use epic_core::pow::Proof;
use epic_core::ser::{self, Error};

fn read_proof(mut bytes: &[u8]) -> Result<Proof, Error> {
	ser::deserialize_default(&mut bytes)
}

#[test]
fn invalid_md5_utf8_returns_error() {
	let invalid_utf8 = [1, 29, 0, 0, 0, 0, 0, 0, 0, 1, 0xff];
	assert_eq!(read_proof(&invalid_utf8), Err(Error::CorruptedData));
}

#[test]
fn unknown_proof_tags_return_errors() {
	for tag in 4..=u8::MAX {
		assert_eq!(read_proof(&[tag]), Err(Error::CorruptedData));
	}
}

#[test]
fn truncated_fixed_size_proofs_return_errors() {
	for tag in [2, 3] {
		for len in 0..32 {
			let mut bytes = vec![0; len + 1];
			bytes[0] = tag;
			assert!(read_proof(&bytes).is_err());
		}
	}
}

#[test]
fn cuckoo_edge_bits_64_returns_error() {
	let mut proof = [0; 338];
	proof[1] = 64;
	assert_eq!(read_proof(&proof), Err(Error::CorruptedData));
}
