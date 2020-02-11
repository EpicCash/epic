// Copyright 2018 The Grin Developers
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
		version: core::HeaderVersion(6),
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

	diff.insert(PoWType::Cuckaroo, 2_u64.pow(2));
	diff.insert(PoWType::Cuckatoo, 2_u64.pow(14));
	diff.insert(PoWType::RandomX, 2_u64.pow(13));
	diff.insert(PoWType::ProgPow, 2_u64.pow(26));

	core::Block::with_header(core::BlockHeader {
		version: core::HeaderVersion(6),
		height: 0,
		timestamp: Utc.ymd(2019, 8, 9).and_hms(17, 04, 38),
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
		output_mmr_size: 0,
		kernel_mmr_size: 0,
		bottles: bottles,
		pow: ProofOfWork {
			total_difficulty: Difficulty::from_dic_number(diff),
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
	})
}

/// Placeholder for mainnet genesis block, will definitely change before
/// release so no use trying to pre-mine it.
pub fn genesis_main() -> core::Block {
	let mut bottles = get_bottles_default();
	bottles.insert(PoWType::Cuckaroo, 1);

	let mut diff = DifficultyNumber::new();

	diff.insert(PoWType::Cuckaroo, 2_u64.pow(2));
	diff.insert(PoWType::Cuckatoo, 2_u64.pow(14));
	diff.insert(PoWType::RandomX, 2_u64.pow(22));
	diff.insert(PoWType::ProgPow, 2_u64.pow(30));

	core::Block::with_header(core::BlockHeader {
		version: core::HeaderVersion(6),
		height: 0,
		timestamp: Utc.ymd(2019, 8, 9).and_hms(17, 04, 38),
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
		output_mmr_size: 0,
		kernel_mmr_size: 0,
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
	})
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::core::hash::Hashed;
	use crate::ser::{self, ProtocolVersion};

	#[test]
	fn floonet_genesis_hash() {
		let gen_hash = genesis_floo().hash();
		println!("floonet genesis hash: {}", gen_hash.to_hex());
		let gen_bin = ser::ser_vec(&genesis_floo(), ProtocolVersion(1)).unwrap();
		println!("floonet genesis full hash: {}\n", gen_bin.hash().to_hex());
		assert_eq!(
			gen_hash.to_hex(),
			"95d457c669f65ee27b0c90b6b11c53d9b51eb0610b6ea8d5c2a45f96d8200c67"
		);
		assert_eq!(
			gen_bin.hash().to_hex(),
			"daab5e09cbcc90a26d60d718afee61a721fe24cbdabf3bfae591f861437b8218"
		);
	}

	// TODO hardcode the hashes once genesis is set
	#[test]
	fn mainnet_genesis_hash() {
		let gen_hash = genesis_main().hash();
		println!("mainnet genesis hash: {}", gen_hash.to_hex());
		let gen_bin = ser::ser_vec(&genesis_main(), ProtocolVersion(1)).unwrap();
		println!("mainnet genesis full hash: {}\n", gen_bin.hash().to_hex());
		assert_eq!(
			gen_hash.to_hex(),
			"454018a56d86e37611bdcabc7de670305c3f3dc9675e314b437f1adc29430851"
		);
		assert_eq!(
			gen_bin.hash().to_hex(),
			"509c7dad7096678942abf510f9c07aecb457041450ff418b531ea4a0f1d67ef4"
		);
	}
}
