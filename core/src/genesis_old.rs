// Copyright 2018 The EPIC Developers
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

//! Definition of the genesis block. Placeholder for now.

// required for genesis replacement
//! #![allow(unused_imports)]

use chrono::prelude::{TimeZone, Utc};

use crate::core;
use crate::global;
use crate::pow::{Difficulty, Proof, ProofOfWork};
use crate::util;
use crate::util::secp::constants::SINGLE_BULLET_PROOF_SIZE;
use crate::util::secp::pedersen::{Commitment, RangeProof};
use crate::util::secp::Signature;

use crate::core::hash::Hash;
use crate::keychain::BlindingFactor;

/// Genesis block definition for development networks. The proof of work size
/// is small enough to mine it on the fly, so it does not contain its own
/// proof of work solution. Can also be easily mutated for different tests.
pub fn genesis_dev() -> core::Block {
	core::Block::with_header(core::BlockHeader {
		height: 0,
		// previous: core::hash::Hash([0xff; 32]),
		timestamp: Utc.ymd(1997, 8, 4).and_hms(0, 0, 0),
		pow: ProofOfWork {
			nonce: global::get_genesis_nonce(),
			..Default::default()
		},
		..Default::default()
	})
}

/// Placeholder for floonet genesis block, will definitely change before
/// release
pub fn genesis_floo() -> core::Block {
	let gen = core::Block::with_header(core::BlockHeader {
		height: 0,
		timestamp: Utc.ymd(2018, 12, 28).and_hms(20, 48, 4),
		prev_root: Hash::from_hex(
			"00000000000000000017ff4903ef366c8f62e3151ba74e41b8332a126542f538",
		)
		.unwrap(),
		output_root: Hash::from_hex(
			"73b5e0a05ea9e1e4e33b8f1c723bc5c10d17f07042c2af7644f4dbb61f4bc556",
		)
		.unwrap(),
		range_proof_root: Hash::from_hex(
			"667a3ba22f237a875f67c9933037c8564097fa57a3e75be507916de28fc0da26",
		)
		.unwrap(),
		kernel_root: Hash::from_hex(
			"cfdddfe2d938d0026f8b1304442655bbdddde175ff45ddf44cb03bcb0071a72d",
		)
		.unwrap(),
		total_kernel_offset: BlindingFactor::from_hex(
			"0000000000000000000000000000000000000000000000000000000000000000",
		)
		.unwrap(),
		output_mmr_size: 1,
		kernel_mmr_size: 1,
		pow: ProofOfWork {
			total_difficulty: Difficulty::from_num(10_u64.pow(5)),
			secondary_scaling: 1856,
			nonce: 23,
			proof: Proof {
				nonces: vec![
					16994232, 22975978, 32664019, 44016212, 50238216, 57272481, 85779161,
					124272202, 125203242, 133907662, 140522149, 145870823, 147481297, 164952795,
					177186722, 183382201, 197418356, 211393794, 239282197, 239323031, 250757611,
					281414565, 305112109, 308151499, 357235186, 374041407, 389924708, 390768911,
					401322239, 401886855, 406986280, 416797005, 418935317, 429007407, 439527429,
					484809502, 486257104, 495589543, 495892390, 525019296, 529899691, 531685572,
				],
				edge_bits: 29,
			},
		},
		..Default::default()
	});
	let kernel = core::TxKernel {
		features: core::KernelFeatures::Coinbase,
		fee: 0,
		lock_height: 0,
		excess: Commitment::from_vec(
			util::from_hex(
				"08df2f1d996cee37715d9ac0a0f3b13aae508d1101945acb8044954aee30960be9".to_string(),
			)
			.unwrap(),
		),
		excess_sig: Signature::from_raw_data(&[
			25, 176, 52, 246, 172, 1, 12, 220, 247, 111, 73, 101, 13, 16, 157, 130, 110, 196, 123,
			217, 246, 137, 45, 110, 106, 186, 0, 151, 255, 193, 233, 178, 103, 26, 210, 215, 200,
			89, 146, 188, 9, 161, 28, 212, 227, 143, 82, 54, 5, 223, 16, 65, 237, 132, 196, 241,
			39, 76, 133, 45, 252, 131, 88, 0,
		])
		.unwrap(),
	};
	let output = core::Output {
		features: core::OutputFeatures::Coinbase,
		commit: Commitment::from_vec(
			util::from_hex(
				"08c12007af16d1ee55fffe92cef808c77e318dae70c3bc70cb6361f49d517f1b68".to_string(),
			)
			.unwrap(),
		),
		proof: RangeProof {
			plen: SINGLE_BULLET_PROOF_SIZE,
			proof: [
				159, 156, 202, 179, 128, 169, 14, 227, 176, 79, 118, 180, 62, 164, 2, 234, 123, 30,
				77, 126, 232, 124, 42, 186, 239, 208, 21, 217, 228, 246, 148, 74, 100, 25, 247,
				251, 82, 100, 37, 16, 146, 122, 164, 5, 2, 165, 212, 192, 221, 167, 199, 8, 231,
				149, 158, 216, 194, 200, 62, 15, 53, 200, 188, 207, 0, 79, 211, 88, 194, 211, 54,
				1, 206, 53, 72, 118, 155, 184, 233, 166, 245, 224, 16, 254, 209, 235, 153, 85, 53,
				145, 33, 186, 218, 118, 144, 35, 189, 241, 63, 229, 52, 237, 231, 39, 176, 202, 93,
				247, 85, 131, 16, 193, 247, 180, 33, 138, 255, 102, 190, 213, 129, 174, 182, 167,
				3, 126, 184, 221, 99, 114, 238, 219, 157, 125, 230, 179, 160, 89, 202, 230, 16, 91,
				199, 57, 158, 225, 142, 125, 12, 211, 164, 78, 9, 4, 155, 106, 157, 41, 233, 188,
				237, 205, 184, 53, 0, 190, 24, 215, 42, 44, 184, 120, 58, 196, 198, 190, 114, 50,
				98, 240, 15, 213, 77, 163, 24, 3, 212, 125, 93, 175, 169, 249, 24, 27, 191, 113,
				89, 59, 169, 40, 87, 250, 144, 159, 118, 171, 232, 92, 217, 5, 179, 152, 249, 247,
				71, 239, 26, 180, 82, 177, 226, 132, 185, 3, 33, 162, 120, 98, 87, 109, 57, 100,
				202, 162, 57, 230, 44, 31, 63, 213, 30, 222, 241, 78, 162, 118, 120, 70, 196, 128,
				72, 223, 110, 5, 17, 151, 97, 214, 43, 57, 157, 1, 59, 87, 96, 17, 159, 174, 144,
				217, 159, 87, 36, 113, 41, 155, 186, 252, 162, 46, 22, 80, 133, 3, 113, 248, 11,
				118, 144, 155, 188, 77, 166, 40, 119, 107, 15, 233, 47, 47, 101, 77, 167, 141, 235,
				148, 34, 218, 164, 168, 71, 20, 239, 71, 24, 12, 109, 146, 232, 243, 65, 31, 72,
				186, 131, 190, 43, 227, 157, 41, 49, 126, 136, 51, 41, 50, 213, 37, 186, 223, 87,
				248, 34, 43, 132, 34, 0, 143, 75, 79, 43, 74, 183, 26, 2, 168, 53, 203, 208, 159,
				69, 107, 124, 33, 68, 113, 206, 127, 216, 158, 15, 52, 206, 1, 101, 109, 199, 13,
				131, 122, 29, 131, 133, 125, 219, 70, 69, 144, 133, 68, 233, 67, 203, 132, 160,
				143, 101, 84, 110, 15, 175, 111, 124, 24, 185, 222, 154, 238, 77, 241, 105, 8, 224,
				230, 43, 178, 49, 95, 137, 33, 227, 118, 207, 239, 56, 21, 51, 220, 22, 48, 162,
				22, 118, 229, 215, 248, 112, 198, 126, 180, 27, 161, 237, 56, 2, 220, 129, 126, 11,
				104, 8, 133, 190, 162, 204, 3, 63, 249, 173, 210, 152, 252, 143, 157, 79, 228, 232,
				230, 72, 164, 131, 183, 151, 230, 219, 186, 21, 34, 154, 219, 215, 231, 179, 47,
				217, 44, 115, 203, 157, 35, 195, 113, 235, 194, 102, 96, 205, 24, 221, 213, 147,
				120, 178, 221, 153, 146, 44, 172, 131, 77, 21, 61, 15, 5, 6, 205, 164, 203, 76,
				228, 29, 126, 136, 88, 230, 210, 62, 164, 103, 125, 55, 231, 129, 89, 61, 222, 50,
				71, 71, 75, 230, 70, 80, 85, 193, 136, 183, 222, 146, 46, 235, 0, 222, 118, 32, 70,
				85, 39, 92, 233, 211, 169, 159, 207, 145, 13, 206, 125, 3, 45, 51, 64, 167, 179,
				133, 83, 57, 190, 51, 239, 211, 74, 116, 75, 71, 248, 249, 184, 13, 31, 129, 107,
				104, 179, 76, 194, 186, 4, 13, 122, 167, 254, 126, 153, 50, 8, 1, 200, 203, 213,
				230, 217, 97, 105, 50, 208, 126, 180, 113, 81, 152, 238, 123, 157, 232, 19, 164,
				159, 164, 89, 75, 33, 70, 140, 204, 158, 236, 10, 226, 102, 14, 88, 134, 82, 131,
				36, 195, 127, 158, 81, 252, 223, 165, 11, 52, 105, 245, 245, 228, 235, 168, 175,
				52, 175, 76, 157, 120, 208, 99, 135, 210, 81, 114, 230, 181,
			],
		},
	};
	gen.with_reward(output, kernel)
}

/// Placeholder for mainnet genesis block, will definitely change before
/// release so no use trying to pre-mine it.
pub fn genesis_main() -> core::Block {
	let gen = core::Block::with_header(core::BlockHeader {
		height: 0,
timestamp: Utc.ymd(2019, 2, 17).and_hms(21, 16, 7),
prev_root: Hash::from_hex("0000000000000000001b3b287dfb8920ca5614a3d353ca19270c4ee75b0df001").unwrap(),
output_root: Hash::from_hex("52a31b532a35e1df68c7c0a68d0e21dbe3ae06da332edba397eaea5d388319f3").unwrap(),
range_proof_root: Hash::from_hex("74f2bca37bbcf648c4ff0fa41803a6d6ecd83b9192b6ba55e2728bcd8050f5b3").unwrap(),
kernel_root: Hash::from_hex("c8b55c99628f27d9ba30c6018675691a4f291f964ea3df0eda72792d1bfd3bf3").unwrap(),
total_kernel_offset: BlindingFactor::from_hex("0000000000000000000000000000000000000000000000000000000000000000").unwrap(),
		output_mmr_size: 1,
		kernel_mmr_size: 1,
		pow: ProofOfWork {
			total_difficulty: Difficulty::from_num(10_u64.pow(2)),
			secondary_scaling: 1856,
nonce: 40,
			proof: Proof {
nonces: vec![168965, 5629691, 9106214, 32138583, 44575382, 72621054, 78000344, 78348935, 80783462, 100349128, 109498448, 123865754, 155318797, 193334365, 204158463, 220674028, 243709304, 269870604, 275728755, 293274566, 305907840, 315211977, 322799736, 329946378, 354880621, 355115577, 357168184, 400565222, 404321697, 406017295, 408981519, 419180059, 428552250, 431139033, 431628511, 446149734, 456047676, 498419287, 502934758, 508440117, 529140221, 534845263],
				edge_bits: 29,
			},
		},
		..Default::default()
	});
	let kernel = core::TxKernel {
		features: core::KernelFeatures::Coinbase,
		fee: 0,
		lock_height: 0,
excess: Commitment::from_vec(util::from_hex("09d91e32722e3b66d645538252a14ff7cc512bff1b1552eeb89d314b0fd4b7277b".to_string()).unwrap()),
excess_sig: Signature::from_raw_data(&[17, 62, 248, 52, 119, 138, 238, 197, 158, 66, 197, 189, 45, 212, 198, 194, 50, 241, 40, 61, 122, 207, 211, 81, 16, 11, 126, 167, 0, 142, 72, 60, 139, 116, 31, 236, 64, 92, 8, 6, 115, 178, 172, 45, 92, 159, 45, 14, 96, 15, 91, 96, 196, 88, 255, 244, 113, 51, 43, 102, 42, 70, 238, 212]).unwrap(),
	};
	let output = core::Output {
		features: core::OutputFeatures::Coinbase,
commit: Commitment::from_vec(util::from_hex("08f61c870bc00c74a8b50e7aaf64c3374dbc119d3b779745d637390e26544ee689".to_string()).unwrap()),
		proof: RangeProof {
			plen: SINGLE_BULLET_PROOF_SIZE,
proof: [47, 239, 150, 186, 199, 19, 103, 180, 95, 234, 230, 177, 13, 159, 54, 6, 13, 66, 90, 163, 108, 229, 175, 121, 147, 3, 103, 252, 173, 17, 196, 85, 174, 115, 29, 45, 151, 244, 6, 139, 204, 148, 154, 4, 231, 8, 76, 238, 22, 151, 74, 102, 50, 38, 245, 2, 55, 114, 140, 2, 210, 92, 130, 9, 4, 8, 117, 31, 79, 84, 173, 193, 135, 104, 116, 46, 98, 12, 175, 47, 208, 30, 23, 134, 248, 238, 67, 252, 43, 110, 99, 193, 191, 208, 243, 104, 85, 71, 146, 74, 255, 5, 155, 255, 238, 99, 214, 69, 184, 143, 45, 137, 249, 138, 176, 51, 1, 242, 202, 40, 183, 100, 235, 42, 125, 252, 200, 243, 192, 182, 211, 216, 50, 208, 107, 142, 235, 88, 211, 156, 158, 248, 178, 197, 231, 117, 199, 79, 185, 142, 142, 10, 160, 28, 66, 146, 249, 40, 210, 192, 253, 4, 232, 231, 116, 64, 94, 10, 54, 215, 209, 234, 1, 127, 25, 15, 226, 111, 166, 95, 119, 176, 239, 144, 249, 79, 18, 5, 175, 7, 254, 143, 36, 196, 187, 56, 194, 136, 218, 147, 138, 190, 135, 166, 198, 148, 123, 31, 73, 56, 106, 6, 84, 91, 231, 46, 39, 172, 223, 39, 60, 187, 134, 103, 186, 65, 52, 44, 150, 215, 74, 228, 104, 136, 186, 205, 218, 246, 145, 118, 77, 198, 113, 81, 40, 121, 7, 114, 167, 155, 205, 81, 24, 97, 127, 37, 165, 83, 123, 107, 179, 105, 113, 97, 110, 202, 60, 243, 55, 142, 137, 48, 139, 141, 140, 137, 194, 150, 57, 182, 147, 199, 226, 96, 76, 2, 187, 219, 27, 219, 114, 21, 250, 181, 120, 215, 52, 197, 14, 136, 176, 52, 114, 241, 60, 211, 228, 18, 193, 193, 87, 103, 91, 161, 207, 78, 135, 225, 135, 108, 139, 220, 175, 197, 70, 196, 250, 78, 36, 149, 74, 115, 70, 3, 245, 229, 189, 233, 14, 44, 55, 6, 34, 120, 252, 31, 226, 116, 15, 110, 249, 157, 72, 5, 2, 42, 82, 209, 190, 136, 174, 126, 216, 110, 255, 193, 112, 153, 58, 198, 137, 148, 173, 71, 88, 164, 110, 60, 119, 62, 8, 42, 132, 171, 123, 19, 186, 157, 47, 7, 203, 3, 92, 27, 151, 123, 113, 24, 27, 143, 235, 96, 119, 129, 6, 65, 196, 29, 21, 230, 34, 220, 8, 36, 156, 123, 102, 37, 78, 183, 177, 168, 225, 212, 233, 59, 63, 17, 4, 184, 225, 39, 28, 59, 34, 25, 165, 93, 51, 20, 187, 48, 4, 18, 99, 99, 23, 129, 79, 196, 100, 31, 64, 217, 218, 139, 191, 99, 121, 188, 162, 130, 73, 28, 31, 33, 206, 43, 44, 165, 133, 186, 149, 48, 170, 43, 42, 85, 147, 227, 166, 81, 26, 215, 92, 58, 72, 196, 150, 153, 147, 83, 243, 52, 68, 91, 15, 115, 56, 33, 91, 128, 101, 35, 249, 89, 219, 68, 30, 216, 46, 227, 141, 45, 27, 123, 3, 6, 240, 169, 119, 16, 31, 162, 100, 197, 51, 224, 119, 93, 233, 201, 146, 252, 115, 104, 155, 19, 73, 150, 209, 227, 191, 152, 2, 154, 44, 128, 226, 233, 171, 77, 121, 198, 193, 124, 17, 130, 179, 118, 239, 24, 110, 153, 163, 106, 22, 27, 194, 87, 108, 155, 21, 223, 111, 1, 88, 151, 204, 190, 35, 55, 120, 200, 53, 251, 140, 253, 96, 198, 173, 168, 204, 245, 82, 218, 42, 30, 156, 31, 44, 128, 252, 44, 45, 67, 71, 179, 108, 133, 245, 137, 104, 136, 49, 209, 251, 224, 95, 76, 30, 225, 68, 187, 31, 146, 78, 125, 228, 221, 194, 72, 158, 120, 255, 230, 81, 3, 53, 49, 164, 6, 59, 255, 53, 34, 243, 138, 57, 255, 89, 175, 114, 88, 161, 80, 135, 249, 65, 139, 225, 167, 164, 157, 91, 226, 11, 143, 244, 136, 148, 81, 92, 76, 201],
		},
	};
	gen.with_reward(output, kernel)
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::core::hash::Hashed;
	use crate::ser;

	#[test]
	fn floonet_genesis_hash() {
		let gen_hash = genesis_floo().hash();
		println!("floonet genesis hash: {}", gen_hash.to_hex());
		let gen_bin = ser::ser_vec(&genesis_floo()).unwrap();
		println!("floonet genesis full hash: {}\n", gen_bin.hash().to_hex());
		assert_eq!(
			gen_hash.to_hex(),
			"edc758c1370d43e1d733f70f58cf187c3be8242830429b1676b89fd91ccf2dab"
		);
		assert_eq!(
			gen_bin.hash().to_hex(),
			"91c638fc019a54e6652bd6bb3d9c5e0c17e889cef34a5c28528e7eb61a884dc4"
		);
	}

	// TODO hardcode the hashes once genesis is set
	#[test]
	fn mainnet_genesis_hash() {
		let gen_hash = genesis_main().hash();
		println!("mainnet genesis hash: {}", gen_hash.to_hex());
		let gen_bin = ser::ser_vec(&genesis_main()).unwrap();
		println!("mainnet genesis full hash: {}\n", gen_bin.hash().to_hex());
		//assert_eq!(gene_hash.to_hex, "");
	}
}
