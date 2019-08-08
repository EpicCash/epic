// Copyright 2018 The Epic Foundation
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
use crate::core::block::feijoada::get_bottles_default;
use crate::global;
use crate::pow::PoWType;
use crate::pow::{Difficulty, DifficultyNumber, Proof, ProofOfWork};
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
	let mut bottles = get_bottles_default();
	bottles.insert(PoWType::Cuckaroo, 1);

	let mut diff = DifficultyNumber::new();

	diff.insert(PoWType::Cuckaroo, 2_u64.pow(1));
	diff.insert(PoWType::Cuckatoo, 2_u64.pow(1));
	diff.insert(PoWType::RandomX, 2_u64.pow(16));
	diff.insert(PoWType::ProgPow, 2_u64.pow(16));

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
		output_mmr_size: 2,
		kernel_mmr_size: 2,
		bottles: bottles,
		pow: ProofOfWork {
			total_difficulty: Difficulty::from_num(10_u64.pow(1)),
			secondary_scaling: 1856,
			nonce: 23,
			proof: Proof::CuckooProof {
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
			seed: [0; 32],
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
	let mut bottles = get_bottles_default();
	bottles.insert(PoWType::Cuckaroo, 1);

	let mut diff = DifficultyNumber::new();

	diff.insert(PoWType::Cuckaroo, 2_u64.pow(1));
	diff.insert(PoWType::Cuckatoo, 2_u64.pow(1));
	diff.insert(PoWType::RandomX, 2_u64.pow(13));
	diff.insert(PoWType::ProgPow, 2_u64.pow(27));

	let gen = core::Block::with_header(core::BlockHeader {
		height: 0,
		timestamp: Utc.ymd(2019, 8, 5).and_hms(17, 19, 38),
		prev_root: Hash::from_hex(
			"00000000000000000004de683e7aa4d35c51f46ec76c6852b0f3161bd1e2e00e",
		)
		.unwrap(),
		output_root: Hash::from_hex(
			"b10fe806a4373d9b8d8edde98a4ec39d726b542036971c2f14c0738b0605c9cd",
		)
		.unwrap(),
		range_proof_root: Hash::from_hex(
			"e05333e51d9294f08cd6d2d7cea19de2843f92c285a61fd5d61d771c3ac74222",
		)
		.unwrap(),
		kernel_root: Hash::from_hex(
			"4d9ddf437dfbb86f8563ac4e96a0d86842eda609a5125244f43261d4188292e4",
		)
		.unwrap(),
		total_kernel_offset: BlindingFactor::from_hex(
			"0000000000000000000000000000000000000000000000000000000000000000",
		)
		.unwrap(),
		output_mmr_size: 1,
		kernel_mmr_size: 1,
		bottles: bottles,
		pow: ProofOfWork {
			total_difficulty: Difficulty::from_dic_number(diff),
			secondary_scaling: 1856,
			nonce: 41,
			proof: Proof::CuckooProof {
				nonces: vec![
					4391451, 36730677, 38198400, 38797304, 60700446, 72910191, 73050441, 110099816,
					140885802, 145512513, 149311222, 149994636, 157557529, 160778700, 162870981,
					179649435, 194194460, 227378628, 230933064, 252046196, 272053956, 277878683,
					288331253, 290266880, 293973036, 305315023, 321927758, 353841539, 356489212,
					373843111, 381697287, 389274717, 403108317, 409994705, 411629694, 431823422,
					441976653, 521469643, 521868369, 523044572, 524964447, 530250249,
				],
				edge_bits: 29,
			},
			seed: [0; 32],
		},
		..Default::default()
	});
	let kernel = core::TxKernel {
		features: core::KernelFeatures::Coinbase,
		fee: 0,
		lock_height: 0,
		excess: Commitment::from_vec(
			util::from_hex(
				"08222aec514ee694fdd3552c15c6649308e0be198bc946583d9eaf01cb6147bbef".to_string(),
			)
			.unwrap(),
		),
		excess_sig: Signature::from_raw_data(&[
			210, 90, 136, 178, 13, 7, 214, 202, 80, 137, 46, 65, 212, 197, 110, 181, 195, 151, 173,
			58, 128, 212, 203, 239, 15, 243, 66, 173, 41, 135, 189, 230, 160, 232, 30, 241, 123,
			222, 40, 106, 0, 3, 65, 53, 156, 238, 80, 89, 83, 110, 9, 182, 81, 222, 221, 209, 119,
			102, 252, 203, 175, 224, 183, 216,
		])
		.unwrap(),
	};
	let output = core::Output {
		features: core::OutputFeatures::Coinbase,
		commit: Commitment::from_vec(
			util::from_hex(
				"095dc61e574f2caa1123fad87cbb0213ea965598c7c6d019e0bc69507469b87875".to_string(),
			)
			.unwrap(),
		),
		proof: RangeProof {
			plen: SINGLE_BULLET_PROOF_SIZE,
			proof: [
				231, 135, 78, 136, 111, 37, 97, 209, 219, 99, 236, 164, 156, 166, 253, 158, 59,
				239, 244, 5, 202, 173, 34, 17, 63, 80, 10, 126, 254, 215, 223, 239, 32, 31, 186,
				26, 39, 227, 148, 3, 20, 160, 213, 88, 242, 236, 51, 10, 132, 162, 153, 132, 89,
				196, 152, 72, 39, 8, 119, 181, 218, 20, 193, 55, 14, 136, 94, 149, 159, 151, 137,
				180, 245, 192, 50, 190, 87, 24, 206, 189, 141, 231, 133, 65, 133, 238, 227, 31, 55,
				55, 218, 239, 32, 15, 47, 116, 66, 71, 131, 58, 105, 160, 124, 228, 66, 122, 246,
				32, 165, 231, 175, 36, 130, 29, 127, 187, 44, 144, 206, 139, 7, 107, 254, 131, 197,
				120, 9, 179, 82, 149, 114, 144, 252, 231, 151, 192, 39, 175, 191, 147, 30, 18, 103,
				219, 12, 23, 53, 205, 113, 249, 13, 135, 69, 111, 102, 118, 1, 226, 236, 32, 238,
				192, 220, 217, 180, 206, 12, 148, 156, 28, 103, 80, 82, 172, 117, 135, 185, 31, 5,
				49, 43, 137, 196, 23, 251, 212, 236, 63, 194, 151, 41, 6, 127, 128, 128, 66, 70,
				121, 203, 38, 134, 141, 213, 66, 122, 48, 193, 36, 183, 9, 179, 122, 160, 66, 200,
				120, 55, 205, 174, 22, 72, 230, 248, 199, 139, 146, 239, 253, 204, 52, 140, 74, 41,
				222, 60, 96, 51, 118, 120, 65, 63, 227, 105, 27, 230, 143, 3, 75, 235, 223, 85, 32,
				118, 27, 93, 1, 176, 115, 164, 158, 163, 117, 235, 36, 235, 77, 27, 155, 132, 67,
				32, 221, 7, 157, 165, 188, 61, 163, 249, 204, 25, 132, 147, 116, 232, 0, 234, 54,
				31, 26, 36, 99, 95, 35, 117, 221, 161, 175, 211, 85, 122, 106, 248, 127, 94, 120,
				255, 73, 240, 74, 220, 155, 61, 43, 120, 55, 23, 215, 140, 106, 118, 173, 200, 223,
				77, 254, 201, 241, 74, 138, 155, 155, 174, 145, 225, 228, 35, 219, 119, 62, 119,
				20, 245, 7, 65, 64, 144, 5, 113, 147, 188, 148, 184, 145, 3, 60, 77, 27, 61, 105,
				182, 126, 4, 252, 8, 126, 155, 2, 187, 27, 165, 143, 43, 185, 158, 135, 117, 144,
				248, 55, 244, 65, 91, 67, 247, 243, 211, 93, 36, 122, 246, 205, 49, 11, 44, 8, 51,
				135, 217, 62, 133, 207, 96, 108, 197, 97, 112, 207, 45, 12, 56, 125, 232, 117, 84,
				109, 171, 183, 204, 71, 61, 130, 228, 187, 225, 213, 143, 98, 28, 58, 195, 67, 20,
				22, 252, 134, 11, 24, 13, 138, 224, 119, 5, 119, 51, 98, 195, 150, 84, 106, 143,
				67, 247, 163, 182, 15, 157, 88, 229, 143, 189, 111, 25, 241, 49, 255, 104, 225, 28,
				20, 236, 209, 67, 104, 188, 39, 102, 132, 17, 4, 117, 252, 190, 142, 240, 58, 143,
				25, 57, 20, 136, 97, 202, 185, 8, 69, 89, 206, 48, 21, 26, 25, 51, 213, 229, 122,
				176, 216, 143, 221, 189, 248, 60, 161, 47, 160, 118, 128, 163, 183, 203, 33, 108,
				174, 190, 84, 127, 5, 166, 233, 177, 113, 24, 19, 113, 74, 114, 80, 72, 133, 235,
				47, 243, 21, 170, 34, 220, 153, 40, 233, 196, 183, 11, 246, 236, 145, 44, 99, 176,
				37, 196, 237, 212, 236, 246, 176, 65, 251, 186, 245, 100, 20, 78, 13, 235, 212, 11,
				199, 176, 70, 37, 48, 90, 16, 171, 222, 1, 112, 197, 169, 167, 148, 155, 108, 112,
				158, 203, 46, 94, 101, 114, 178, 109, 142, 88, 26, 248, 138, 72, 90, 253, 174, 107,
				202, 101, 170, 100, 67, 93, 132, 254, 49, 104, 2, 46, 61, 95, 101, 126, 176, 202,
				41, 18, 111, 193, 182, 212, 251, 179, 233, 181, 67, 72, 100, 212, 236, 190, 245,
				182, 202, 24, 45, 102, 116, 32, 42, 227, 136, 2, 206, 141, 178, 245, 103, 144, 36,
				0, 248, 38, 214, 249, 95, 3, 26, 168,
			],
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
			"09632d856d4927e91fb358cbc813db0a51a1b6c145c7d9c1d2baf7696b24f836"
		);
		assert_eq!(
			gen_bin.hash().to_hex(),
			"265b36e11898c9fd8604585d0228850ac9cdbba6d7aa6da7ef7dfc08b3ead831"
		);
	}

	// TODO hardcode the hashes once genesis is set
	#[test]
	fn mainnet_genesis_hash() {
		let gen_hash = genesis_main().hash();
		println!("mainnet genesis hash: {}", gen_hash.to_hex());
		let gen_bin = ser::ser_vec(&genesis_main()).unwrap();
		println!("mainnet genesis full hash: {}\n", gen_bin.hash().to_hex());
		assert_eq!(
			gen_hash.to_hex(),
			"5f742531d24ea874a6367a81077fe270f7375625ac4951c1aec10e30aaa56cf2"
		);
		assert_eq!(
			gen_bin.hash().to_hex(),
			"89ff35c4adddf9dfd8493525e6e981eb60b67f70f55982f945abc6f4d44425e1"
		);
	}
}
