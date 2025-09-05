use num_bigint::BigUint;
use std::marker::PhantomData;

use crate::pow::common::EdgeType;
use crate::pow::error::Error;
use crate::pow::{PoWContext, Proof};
use crate::util::RwLock;

use randomx::{slow_hash, RxState};

lazy_static! {
	pub static ref RX_STATE: RwLock<RxState> = RwLock::new(RxState::new());
}

pub const SEEDHASH_EPOCH_BLOCKS: u64 = 1000;
pub const SEEDHASH_EPOCH_LAG: u64 = 60;

pub fn rx_epoch_start(epoch_height: u64) -> u64 {
	if epoch_height == 0 {
		0
	} else {
		epoch_height + SEEDHASH_EPOCH_LAG
	}
}

pub fn rx_epoch_end(epoch_height: u64) -> u64 {
	if epoch_height == 0 {
		SEEDHASH_EPOCH_BLOCKS + SEEDHASH_EPOCH_LAG
	} else {
		epoch_height + SEEDHASH_EPOCH_LAG + SEEDHASH_EPOCH_BLOCKS
	}
}

pub fn rx_next_seed_height(height: u64) -> Option<u64> {
	let next_height = height - (height % SEEDHASH_EPOCH_BLOCKS);

	if height <= SEEDHASH_EPOCH_BLOCKS {
		return None;
	}

	if (height - 1) % SEEDHASH_EPOCH_BLOCKS <= SEEDHASH_EPOCH_LAG {
		Some(next_height)
	} else {
		None
	}
}

pub fn rx_current_seed_height(height: u64) -> u64 {
	if height <= SEEDHASH_EPOCH_LAG + SEEDHASH_EPOCH_BLOCKS {
		return 0;
	}

	if height % SEEDHASH_EPOCH_BLOCKS <= SEEDHASH_EPOCH_LAG {
		height - (height % SEEDHASH_EPOCH_BLOCKS) - SEEDHASH_EPOCH_BLOCKS
	} else {
		height - (height % SEEDHASH_EPOCH_BLOCKS)
	}
}

pub struct RXContext<T>
where
	T: EdgeType,
{
	pub seed: [u8; 32],
	pub header: Vec<u8>,
	pub nonce: u64,
	phantom: PhantomData<T>,
}

pub fn new_randomx_ctx<T>(seed: [u8; 32]) -> Result<Box<dyn PoWContext<T>>, Error>
where
	T: EdgeType + 'static,
{
	Ok(Box::new(RXContext {
		phantom: PhantomData,
		header: vec![],
		nonce: 0,
		seed,
	}))
}

impl<T> PoWContext<T> for RXContext<T>
where
	T: EdgeType,
{
	fn set_header_nonce(
		&mut self,
		header: Vec<u8>,
		nonce: Option<u64>,
		_height: Option<u64>,
		_solve: bool,
	) -> Result<(), Error> {
		self.header = header;
		self.nonce = nonce.unwrap_or(self.nonce);
		Ok(())
	}

	fn pow_solve(&mut self) -> Result<Vec<Proof>, Error> {
		let hash: num_bigint::BigUint = {
			let mut state = RX_STATE.write();
			slow_hash(&mut state, &self.header, &self.seed)
		};

		let hash_bytes: [u8; 32] = biguint_to_u8_32(hash);
		Ok(vec![Proof::RandomXProof { hash: hash_bytes }])
	}

	fn verify(&mut self, proof: &Proof) -> Result<(), Error> {
		let hash = {
			let mut state = RX_STATE.write();
			slow_hash(&mut state, &self.header, &self.seed)
		};

		let hash_u8: [u8; 32] = biguint_to_u8_32(hash);

		if let Proof::RandomXProof { hash: ref proof } = proof {
			if &hash_u8 == proof {
				return Ok(());
			}
		}

		Err(Error::Verification("Hash randomx invalid!".to_string()))?
	}
}

fn biguint_to_u8_32(value: BigUint) -> [u8; 32] {
	let mut bytes = [0u8; 32]; // Initialize a 32-byte array with zeros
	let biguint_bytes = value.to_bytes_be(); // Get the big-endian byte representation of the BigUint

	// Copy the bytes into the array, starting from the right (least significant bytes)
	let start = 32usize.saturating_sub(biguint_bytes.len());
	bytes[start..].copy_from_slice(&biguint_bytes[..std::cmp::min(32, biguint_bytes.len())]);

	bytes
}
