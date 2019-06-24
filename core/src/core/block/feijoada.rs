use crate::ser::{self, Readable, Reader, Writeable, Writer};
use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum PoWType {
	Cuckaroo,
	Cuckatoo,
	RandomX,
}

static POW_TYPE_STRING: [&'static str; 3] = ["cuckaroo", "cuckatoo", "randomx"];
static POW_TYPE_VALUE: [PoWType; 3] = [PoWType::Cuckaroo, PoWType::Cuckatoo, PoWType::RandomX];

impl Serialize for PoWType {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		serializer.serialize_str(match *self {
			PoWType::Cuckaroo => "cuckaroo",
			PoWType::Cuckatoo => "cuckatoo",
			PoWType::RandomX => "randomx",
		})
	}
}

impl<'de> Deserialize<'de> for PoWType {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		let hashmap: HashMap<&'static str, PoWType> = POW_TYPE_STRING
			.iter()
			.enumerate()
			.map(|(i, &x)| (x, POW_TYPE_VALUE[i]))
			.collect();
		match hashmap.get(s.as_str()) {
			Some(&x) => Ok(x),
			None => Err(de::Error::unknown_variant(&s, &POW_TYPE_STRING)),
		}
	}
}

/// The configuration for the policy on accepted blocks
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicyConfig {
	pub allowed_policies: u64,
	pub emitted_policy: u8,
	pub policies: Vec<Policy>,
}

impl Default for PolicyConfig {
	fn default() -> Self {
		// default just in tests
		let mut policies = get_bottles_default();
		policies.insert(PoWType::Cuckaroo, 100);

		PolicyConfig {
			allowed_policies: 0,
			emitted_policy: 0,
			policies: vec![policies],
		}
	}
}

/// The ideal proportion each block should have according to the current policy
pub type Policy = HashMap<PoWType, u32>;

pub fn get_bottles_default() -> Policy {
	let mut policy: Policy = Policy::new();
	policy.insert(PoWType::Cuckaroo, 0);
	policy.insert(PoWType::Cuckatoo, 0);
	policy.insert(PoWType::RandomX, 0);
	policy
}

fn next_should_reset(bottle: &Policy) -> bool {
	count_beans(bottle) == 100
}

pub fn next_block_bottles(pow: PoWType, bottle: &Policy) -> Policy {
	let mut new_bottle = if next_should_reset(bottle) {
		HashMap::new()
	} else {
		bottle.clone()
	};
	let entry = new_bottle.entry(pow).or_insert(0);
	*entry += 1;
	new_bottle
}

impl Writeable for Policy {
	fn write<W: Writer>(&self, writer: &mut W) -> Result<(), ser::Error> {
		writer.write_u64(self.len() as u64)?;
		let mut policy_vec: Vec<(PoWType, u32)> = self.iter().map(|(&x, &num)| (x, num)).collect();
		policy_vec.sort();
		for (algo, num) in policy_vec.iter() {
			writer.write_u8(match algo {
				PoWType::Cuckaroo => 0,
				PoWType::Cuckatoo => 1,
				PoWType::RandomX => 2,
			})?;
			writer.write_u32(*num)?;
		}
		Ok(())
	}
}

impl Readable for Policy {
	fn read(reader: &mut dyn Reader) -> Result<Policy, ser::Error> {
		let len = reader.read_u64()?;
		let mut result = HashMap::new();
		for _ in 0..len {
			let pow = match reader.read_u8()? {
				0 => PoWType::Cuckaroo,
				1 => PoWType::Cuckatoo,
				2 => PoWType::RandomX,
				_ => {
					return Err(ser::Error::CorruptedData);
				}
			};
			let count = reader.read_u32()?;
			result.insert(pow, count);
		}
		Ok(result)
	}
}

fn largest_allotment(policy: &Policy) -> PoWType {
	let (algo, _) = policy.iter().max_by(|&(_, x), &(_, y)| x.cmp(y)).unwrap();
	*algo
}

fn check_policy(policy: &Policy) {
	assert_eq!(100, policy.values().fold(0, |acc, &x| x + acc));
}

fn count_beans(bottles: &Policy) -> u32 {
	std::cmp::max(bottles.values().fold(0, |acc, &x| x + acc), 1)
}

pub trait Feijoada {
	fn choose_algo(policy: &Policy, bottles: &Policy) -> PoWType;
}

pub struct Deterministic;

impl Feijoada for Deterministic {
	fn choose_algo(policy: &Policy, bottles: &Policy) -> PoWType {
		let bean_total = count_beans(bottles);
		let scores: HashMap<PoWType, f32> = bottles
			.iter()
			.map(|(&algo, &beans)| (algo, 100.0 * (beans as f32) / (bean_total as f32)))
			.collect();
		*(policy
			.iter()
			.map(|(a, &v)| (a, v as f32 - scores[a]))
			.max_by(|&(_, x), &(_, y)| x.partial_cmp(&y).unwrap())
			.unwrap()
			.0)
	}
}
