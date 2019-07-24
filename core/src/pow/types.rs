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

use std::cmp::Ordering;
/// Types for a Cuck(at)oo proof of work and its encapsulation as a fully usable
/// proof of work within a block header.
use std::cmp::{max, min};
use std::ops::{Add, Div, Mul, Sub};
use std::{fmt, iter};

use rand::{thread_rng, Rng};
use serde::ser::SerializeMap;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use bigint::uint::U256;

use crate::consensus::{graph_weight, MIN_DIFFICULTY, SECOND_POW_EDGE_BITS};
use crate::core::hash::{DefaultHashable, Hashed};
use crate::global;
use crate::ser::{self, FixedLength, Readable, Reader, Writeable, Writer};

use crate::core::hash::Hash;
use crate::pow::common::EdgeType;
use crate::pow::error::Error;
use crate::pow::progpow::get_progpow_value;
use crate::util::read_write::from_slice;

use std::collections::HashMap;

const ALGORITHM_COUNTS: usize = 4;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum PoWType {
	Cuckaroo,
	Cuckatoo,
	RandomX,
	ProgPow,
}

impl PoWType {
	fn to_u8(&self) -> u8 {
		match self {
			PoWType::Cuckaroo => 0,
			PoWType::Cuckatoo => 1,
			PoWType::RandomX => 2,
			PoWType::ProgPow => 3,
		}
	}
}

impl From<u8> for PoWType {
	fn from(v: u8) -> PoWType {
		match v {
			0 => PoWType::Cuckaroo,
			1 => PoWType::Cuckatoo,
			2 => PoWType::RandomX,
			3 => PoWType::ProgPow,
			_ => panic!("data corrupted"),
		}
	}
}

impl From<Proof> for PoWType {
	fn from(v: Proof) -> PoWType {
		match v {
			Proof::CuckooProof { ref edge_bits, .. } => {
				if *edge_bits == 19 || *edge_bits == 31 {
					PoWType::Cuckatoo
				} else {
					PoWType::Cuckaroo
				}
			}
			Proof::RandomXProof { .. } => PoWType::RandomX,
			Proof::ProgPowProof { .. } => PoWType::ProgPow,
			Proof::MD5Proof { .. } => PoWType::RandomX, //panic!("algorithm is not working!"),
		}
	}
}

/// Generic trait for a solver/verifier providing common interface into Cuckoo-family PoW
/// Mostly used for verification, but also for test mining if necessary
pub trait PoWContext<T>
where
	T: EdgeType,
{
	/// Sets the header along with an optional nonce at the end
	/// solve: whether to set up structures for a solve (true) or just validate (false)
	fn set_header_nonce(
		&mut self,
		header: Vec<u8>,
		nonce: Option<u64>,
		height: Option<u64>,
		solve: bool,
	) -> Result<(), Error>;
	/// find solutions using the stored parameters and header
	fn pow_solve(&mut self) -> Result<Vec<Proof>, Error>;
	/// Verify a solution with the stored parameters
	fn verify(&mut self, proof: &Proof) -> Result<(), Error>;
}

pub trait DifficultyNumberBasic {
	fn number(v: u64) -> Self;
}

pub type DifficultyNumber = HashMap<PoWType, u64>;

impl DifficultyNumberBasic for DifficultyNumber {
	fn number(v: u64) -> DifficultyNumber {
		let mut h = HashMap::new();

		h.insert(PoWType::Cuckaroo, v);
		h.insert(PoWType::Cuckatoo, v);
		h.insert(PoWType::RandomX, v);
		h.insert(PoWType::ProgPow, v);

		h
	}
}

/// The difficulty is defined as the maximum target divided by the block hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difficulty {
	pub num: DifficultyNumber,
}

impl Difficulty {
	/// Difficulty of zero, which is invalid (no target can be
	/// calculated from it) but very useful as a start for additions.
	pub fn zero() -> Difficulty {
		Difficulty {
			num: DifficultyNumber::number(0),
		}
	}

	/// Difficulty of MIN_DIFFICULTY
	pub fn min() -> Difficulty {
		Difficulty {
			num: DifficultyNumber::number(MIN_DIFFICULTY),
		}
	}

	/// Difficulty unit, which is the graph weight of minimal graph
	pub fn unit() -> Difficulty {
		Difficulty {
			num: DifficultyNumber::number(global::initial_graph_weight() as u64),
		}
	}

	/// Convert a `u32` into a `Difficulty`
	pub fn from_num(num: u64) -> Difficulty {
		// can't have difficulty lower than 1
		Difficulty {
			num: DifficultyNumber::number(max(num, 1)),
		}
	}

	pub fn from_dic_number(num: DifficultyNumber) -> Difficulty {
		// can't have difficulty lower than 1
		Difficulty { num }
	}

	/// Computes the difficulty from a hash. Divides the maximum target by the
	/// provided hash and applies the Cuck(at)oo size adjustment factor (see
	/// https://lists.launchpad.net/mimblewimble/msg00494.html).
	fn from_proof_adjusted(height: u64, proof: &Proof) -> Difficulty {
		match proof {
			// scale with natural scaling factor
			Proof::CuckooProof { edge_bits, .. } => {
				Difficulty::from_num(proof.scaled_difficulty(graph_weight(height, *edge_bits)))
			}
			Proof::MD5Proof { edge_bits, .. } => {
				Difficulty::from_num(proof.scaled_difficulty(graph_weight(height, *edge_bits)))
			}
			_ => Difficulty::from_num(proof.scaled_difficulty(graph_weight(height, 31))),
		}
	}

	fn from_proof_hash(hash: &[u8; 32]) -> Difficulty {
		let d: U256 = hash.into();
		let result: U256 = U256::max_value() / d;
		Difficulty {
			num: DifficultyNumber::number(result.low_u64()),
		}
	}

	/// Same as `from_proof_adjusted` but instead of an adjustment based on
	/// cycle size, scales based on a provided factor. Used by dual PoW system
	/// to scale one PoW against the other.
	fn from_proof_scaled(proof: &Proof, scaling: u32) -> Difficulty {
		// Scaling between 2 proof of work algos
		Difficulty::from_num(proof.scaled_difficulty(scaling as u64))
	}

	/// Converts the difficulty into a u64
	pub fn to_num(&self, pow: PoWType) -> u64 {
		*self.num.get(&pow).unwrap_or(&0)
	}
}

impl fmt::Display for Difficulty {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let mut diff_vec: Vec<(PoWType, u64)> =
			self.num.iter().map(|(&x, &num)| (x, num)).collect();
		diff_vec.sort();

		write!(f, "{:?}", diff_vec)
	}
}

impl Ord for Difficulty {
	fn cmp(&self, other: &Self) -> Ordering {
		self.num
			.get(&PoWType::Cuckatoo)
			.unwrap()
			.cmp(&other.num.get(&PoWType::Cuckatoo).unwrap())
	}
}

impl PartialOrd for Difficulty {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(
			self.num
				.get(&PoWType::Cuckatoo)
				.unwrap()
				.cmp(&other.num.get(&PoWType::Cuckatoo).unwrap()),
		)
	}
}

impl Add<Difficulty> for Difficulty {
	type Output = Difficulty;
	fn add(self, other: Difficulty) -> Difficulty {
		let mut d = DifficultyNumber::number(0);
		for (algo, v) in &self.num {
			d.insert(*algo, v + *other.num.get(algo).unwrap_or(&0));
		}
		Difficulty { num: d }
	}
}

impl Sub<Difficulty> for Difficulty {
	type Output = Difficulty;
	fn sub(self, other: Difficulty) -> Difficulty {
		let mut d = DifficultyNumber::number(0);
		for (algo, v) in &self.num {
			d.insert(*algo, v - *other.num.get(algo).unwrap_or(&0));
		}
		Difficulty { num: d }
	}
}

impl Mul<Difficulty> for Difficulty {
	type Output = Difficulty;
	fn mul(self, other: Difficulty) -> Difficulty {
		let mut d = DifficultyNumber::number(0);
		for (algo, v) in &self.num {
			d.insert(*algo, v * *other.num.get(algo).unwrap_or(&0));
		}
		Difficulty { num: d }
	}
}

impl Div<Difficulty> for Difficulty {
	type Output = Difficulty;
	fn div(self, other: Difficulty) -> Difficulty {
		let mut d = DifficultyNumber::number(0);
		for (algo, v) in &self.num {
			d.insert(*algo, v / *other.num.get(algo).unwrap_or(&0));
		}
		Difficulty { num: d }
	}
}

impl Writeable for Difficulty {
	fn write<W: Writer>(&self, writer: &mut W) -> Result<(), ser::Error> {
		writer.write_u64(self.num.len() as u64)?;
		let mut diff_vec: Vec<(PoWType, u64)> =
			self.num.iter().map(|(&x, &num)| (x, num)).collect();
		diff_vec.sort();

		for (algo, num) in diff_vec.iter() {
			writer.write_u8(algo.to_u8())?;
			writer.write_u64(*num)?;
		}

		Ok(())
	}
}

impl Readable for Difficulty {
	fn read(reader: &mut dyn Reader) -> Result<Difficulty, ser::Error> {
		let len = reader.read_u64()?;
		let mut result = HashMap::new();
		for _ in 0..len {
			let pow = reader.read_u8()?;
			let count = reader.read_u64()?;
			result.insert(pow.into(), count);
		}
		Ok(Difficulty { num: result })
	}
}

impl FixedLength for Difficulty {
	const LEN: usize = 8 + ALGORITHM_COUNTS * 9;
}

impl Serialize for Difficulty {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		let mut map = serializer.serialize_map(Some(self.num.len()))?;
		let mut diff_vec: Vec<(PoWType, u64)> =
			self.num.iter().map(|(&x, &num)| (x, num)).collect();
		diff_vec.sort();

		for (algo, num) in &diff_vec {
			map.serialize_entry(&algo, num)?;
		}

		map.end()
	}
}

impl<'de> Deserialize<'de> for Difficulty {
	fn deserialize<D>(deserializer: D) -> Result<Difficulty, D::Error>
	where
		D: Deserializer<'de>,
	{
		deserializer.deserialize_map(DifficultyMap)
	}
}

struct DifficultyMap;

impl<'de> de::Visitor<'de> for DifficultyMap {
	type Value = Difficulty;

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		formatter.write_str("a difficulty map")
	}

	fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
	where
		M: de::MapAccess<'de>,
	{
		let mut map = DifficultyNumber::with_capacity(access.size_hint().unwrap_or(0));

		while let Some((key, value)) = access.next_entry()? {
			map.insert(key, value);
		}

		Ok(Difficulty { num: map })
	}
}

/// Block header information pertaining to the proof of work
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProofOfWork {
	/// Total accumulated difficulty since genesis block
	pub total_difficulty: Difficulty,
	/// Variable difficulty scaling factor fo secondary proof of work
	pub secondary_scaling: u32,
	/// Nonce increment used to mine this block.
	pub nonce: u64,
	/// Proof of work data.
	pub proof: Proof,
	/// Randomx seed.
	pub seed: [u8; 32],
}

impl Default for ProofOfWork {
	fn default() -> ProofOfWork {
		let proof_size = global::proofsize();
		ProofOfWork {
			total_difficulty: Difficulty::from_num(1),
			secondary_scaling: 1,
			nonce: 0,
			proof: Proof::zero(proof_size),
			seed: [0; 32],
		}
	}
}

impl Writeable for ProofOfWork {
	fn write<W: Writer>(&self, writer: &mut W) -> Result<(), ser::Error> {
		if writer.serialization_mode() != ser::SerializationMode::Hash {
			self.write_pre_pow(writer)?;
			writer.write_u64(self.nonce)?;
		}
		self.proof.write(writer)?;
		Ok(())
	}
}

impl Readable for ProofOfWork {
	fn read(reader: &mut dyn Reader) -> Result<ProofOfWork, ser::Error> {
		let total_difficulty = Difficulty::read(reader)?;
		let secondary_scaling = reader.read_u32()?;

		let nonce = reader.read_u64()?;
		let proof = Proof::read(reader)?;

		let seed_bytes = reader.read_fixed_bytes(32)?;
		let seed: [u8; 32] = from_slice(&seed_bytes);

		Ok(ProofOfWork {
			total_difficulty,
			secondary_scaling,
			nonce,
			proof,
			seed,
		})
	}
}

impl ProofOfWork {
	/// Write implementation, can't define as trait impl as we need a version
	pub fn write<W: Writer>(&self, writer: &mut W) -> Result<(), ser::Error> {
		if writer.serialization_mode() != ser::SerializationMode::Hash {
			self.write_pre_pow(writer)?;
			writer.write_u64(self.nonce)?;
		}

		self.proof.write(writer)?;
		writer.write_fixed_bytes(&self.seed)?;
		Ok(())
	}

	/// Write the pre-hash portion of the header
	pub fn write_pre_pow<W: Writer>(&self, writer: &mut W) -> Result<(), ser::Error> {
		self.total_difficulty.write(writer)?;
		writer.write_u32(self.secondary_scaling)
	}

	/// Maximum difficulty this proof of work can achieve
	pub fn to_difficulty(&self, header: &[u8], height: u64, nonce: u64) -> Difficulty {
		match self.proof {
			Proof::CuckooProof { edge_bits, .. } => {
				// 2 proof of works, Cuckoo29 (for now) and Cuckoo30+, which are scaled
				// differently (scaling not controlled for now)
				if edge_bits == SECOND_POW_EDGE_BITS {
					Difficulty::from_proof_scaled(&self.proof, self.secondary_scaling)
				} else {
					Difficulty::from_proof_adjusted(height, &self.proof)
				}
			}
			Proof::MD5Proof { edge_bits, .. } => {
				if edge_bits == SECOND_POW_EDGE_BITS {
					Difficulty::from_proof_scaled(&self.proof, self.secondary_scaling)
				} else {
					Difficulty::from_proof_adjusted(height, &self.proof)
				}
			}
			Proof::RandomXProof { ref hash } => Difficulty::from_proof_hash(hash),
			Proof::ProgPowProof { ref mix } => {
				Difficulty::from_proof_hash(&get_progpow_value(header, height, nonce))
			}
		}
	}

	/// The edge_bits used for the cuckoo cycle size on this proof
	pub fn edge_bits(&self) -> u8 {
		match self.proof {
			Proof::CuckooProof { edge_bits, .. } => edge_bits,
			Proof::MD5Proof { edge_bits, .. } => edge_bits,
			_ => 16,
		}
	}

	/// Whether this proof of work is for the primary algorithm (as opposed
	/// to secondary). Only depends on the edge_bits at this time.
	pub fn is_primary(&self) -> bool {
		match self.proof {
			Proof::CuckooProof { edge_bits, .. } => {
				// 2 conditions are redundant right now but not necessarily in
				// the future
				edge_bits != SECOND_POW_EDGE_BITS && edge_bits >= global::min_edge_bits()
			}
			Proof::MD5Proof { edge_bits, .. } => {
				edge_bits != SECOND_POW_EDGE_BITS && edge_bits >= global::min_edge_bits()
			}
			_ => true,
		}
	}

	/// Whether this proof of work is for the secondary algorithm (as opposed
	/// to primary). Only depends on the edge_bits at this time.
	pub fn is_secondary(&self) -> bool {
		match self.proof {
			Proof::CuckooProof { edge_bits, .. } => edge_bits == SECOND_POW_EDGE_BITS,
			Proof::MD5Proof { edge_bits, .. } => edge_bits == SECOND_POW_EDGE_BITS,
			_ => false,
		}
	}
}

/// A proof of work
#[derive(Clone, PartialOrd, PartialEq, Serialize)]
pub enum Proof {
	/// A Cuck(at)oo Cycle proof of work, consisting of the edge_bits to get the graph
	/// size (i.e. the 2-log of the number of edges) and the nonces
	/// of the graph solution. While being expressed as u64 for simplicity,
	/// nonces a.k.a. edge indices range from 0 to (1 << edge_bits) - 1
	///
	/// The hash of the `Proof` is the hash of its packed nonces when serializing
	/// them at their exact bit size. The resulting bit sequence is padded to be
	/// byte-aligned.
	///
	CuckooProof {
		/// Power of 2 used for the size of the cuckoo graph
		edge_bits: u8,
		/// The nonces
		nonces: Vec<u64>,
	},

	MD5Proof {
		proof: String,
		edge_bits: u8,
	},

	RandomXProof {
		hash: [u8; 32],
	},

	ProgPowProof {
		mix: [u8; 32],
	},
}

impl DefaultHashable for Proof {}

impl fmt::Debug for Proof {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Proof::CuckooProof { edge_bits, nonces } => {
				write!(f, "Cuckoo{}(", *edge_bits)?;
				for (i, val) in nonces[..].iter().enumerate() {
					write!(f, "{:x}", val)?;
					if i < nonces.len() - 1 {
						write!(f, " ")?;
					}
				}
				write!(f, ")")
			}
			Proof::MD5Proof { ref proof, .. } => write!(f, "MD5 ({})", proof),
			Proof::RandomXProof { ref hash } => {
				let hash: U256 = hash.into();
				write!(f, "RandomX ({})", hash)
			}
			Proof::ProgPowProof { ref mix } => write!(f, "Progpow: mix({:?})", mix),
		}
	}
}

impl Eq for Proof {}

impl Proof {
	/// Builds a proof with provided nonces at default edge_bits
	pub fn new(mut in_nonces: Vec<u64>) -> Proof {
		in_nonces.sort_unstable();
		Proof::CuckooProof {
			edge_bits: global::min_edge_bits(),
			nonces: in_nonces,
		}
	}

	/// Builds a proof with all bytes zeroed out
	pub fn zero(proof_size: usize) -> Proof {
		Proof::CuckooProof {
			edge_bits: global::min_edge_bits(),
			nonces: vec![0; proof_size],
		}
	}

	/// Builds a proof with random POW data,
	/// needed so that tests that ignore POW
	/// don't fail due to duplicate hashes
	pub fn random(proof_size: usize) -> Proof {
		let edge_bits = global::min_edge_bits();
		let nonce_mask = (1 << edge_bits) - 1;
		let mut rng = thread_rng();
		// force the random num to be within edge_bits bits
		let mut v: Vec<u64> = iter::repeat(())
			.map(|()| (rng.gen::<u32>() & nonce_mask) as u64)
			.take(proof_size)
			.collect();
		v.sort_unstable();
		Proof::CuckooProof {
			edge_bits: global::min_edge_bits(),
			nonces: v,
		}
	}

	/// Returns the proof size
	pub fn proof_size(&self) -> usize {
		match self {
			Proof::CuckooProof { nonces, .. } => nonces.len(),
			Proof::MD5Proof { .. } => 16,
			_ => 16,
		}
	}

	/// Difficulty achieved by this proof with given scaling factor
	fn scaled_difficulty(&self, scale: u64) -> u64 {
		let diff = ((scale as u128) << 64) / (max(1, self.hash().to_u64()) as u128);
		min(diff, <u64>::max_value() as u128) as u64
	}
}

impl Readable for Proof {
	fn read(reader: &mut dyn Reader) -> Result<Proof, ser::Error> {
		let pow_type = reader.read_u8()?;
		match pow_type {
			0 => {
				let edge_bits = reader.read_u8()?;
				if edge_bits == 0 || edge_bits > 64 {
					return Err(ser::Error::CorruptedData);
				}

				// prepare nonces and read the right number of bytes
				let mut nonces = Vec::with_capacity(global::proofsize());
				let nonce_bits = edge_bits as usize;
				let bits_len = nonce_bits * global::proofsize();
				let bytes_len = BitVec::bytes_len(bits_len);
				let bits = reader.read_fixed_bytes(bytes_len)?;

				// set our nonces from what we read in the bitvec
				let bitvec = BitVec { bits };
				for n in 0..global::proofsize() {
					let mut nonce = 0;
					for bit in 0..nonce_bits {
						if bitvec.bit_at(n * nonce_bits + (bit as usize)) {
							nonce |= 1 << bit;
						}
					}
					nonces.push(nonce);
				}

				// check the last bits of the last byte are zeroed, we don't use them but
				// still better to enforce to avoid any malleability
				for n in bits_len..(bytes_len * 8) {
					if bitvec.bit_at(n) {
						return Err(ser::Error::CorruptedData);
					}
				}

				Ok(Proof::CuckooProof { edge_bits, nonces })
			}
			1 => {
				let edge_bits = reader.read_u8()?;
				let proof_bytes = reader.read_bytes_len_prefix()?;
				let proof = std::str::from_utf8(&proof_bytes).unwrap().to_string();
				Ok(Proof::MD5Proof { edge_bits, proof })
			}
			2 => {
				let hash = from_slice(&reader.read_fixed_bytes(32).unwrap());
				Ok(Proof::RandomXProof { hash })
			}
			3 => {
				let mix = from_slice(&reader.read_fixed_bytes(32).unwrap());
				//let value = from_slice(&reader.read_fixed_bytes(32).unwrap());
				Ok(Proof::ProgPowProof { mix })
			}
			_ => panic!("Unknown byte"),
		}
	}
}

impl Writeable for Proof {
	fn write<W: Writer>(&self, writer: &mut W) -> Result<(), ser::Error> {
		match self {
			Proof::CuckooProof { edge_bits, nonces } => {
				writer.write_u8(0)?;
				if writer.serialization_mode() != ser::SerializationMode::Hash {
					writer.write_u8(*edge_bits)?;
				}
				let nonce_bits = *edge_bits as usize;
				let mut bitvec = BitVec::new(nonce_bits * global::proofsize());
				for (n, nonce) in nonces.iter().enumerate() {
					for bit in 0..nonce_bits {
						if nonce & (1 << bit) != 0 {
							bitvec.set_bit_at(n * nonce_bits + (bit as usize))
						}
					}
				}
				writer.write_fixed_bytes(&bitvec.bits)?;
				Ok(())
			}
			Proof::MD5Proof {
				ref proof,
				edge_bits,
			} => {
				writer.write_u8(1)?;
				writer.write_u8(*edge_bits)?;
				writer.write_bytes(&proof.as_ref())?;
				Ok(())
			}
			Proof::RandomXProof { ref hash } => {
				writer.write_u8(2)?;
				writer.write_fixed_bytes(hash)?;
				Ok(())
			}
			Proof::ProgPowProof { ref mix } => {
				writer.write_u8(3)?;
				writer.write_fixed_bytes(mix)?;
				//writer.write_fixed_bytes(value)?;
				Ok(())
			}
		}
	}
}

// TODO this could likely be optimized by writing whole bytes (or even words)
// in the `BitVec` at once, dealing with the truncation, instead of bits by bits
struct BitVec {
	bits: Vec<u8>,
}

impl BitVec {
	/// Number of bytes required to store the provided number of bits
	fn bytes_len(bits_len: usize) -> usize {
		(bits_len + 7) / 8
	}

	fn new(bits_len: usize) -> BitVec {
		BitVec {
			bits: vec![0; BitVec::bytes_len(bits_len)],
		}
	}

	fn set_bit_at(&mut self, pos: usize) {
		self.bits[pos / 8] |= 1 << (pos % 8) as u8;
	}

	fn bit_at(&self, pos: usize) -> bool {
		self.bits[pos / 8] & (1 << (pos % 8) as u8) != 0
	}
}
