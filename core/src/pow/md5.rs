//! Implementation of MD5 by Yuri Albuquerque
use crate::pow::common::{EdgeType, Link};
use crate::pow::error::{Error, ErrorKind};
use crate::pow::{PoWContext, Proof};
use crate::util;
use std::marker::PhantomData;

pub struct MD5Context<T>
where
	T: EdgeType,
{
	phantom: PhantomData<T>,
	pub edge_bits: u8,
	pub proof_size: usize,
	pub max_sols: u32,
	header: Vec<u8>,
	nonce: u64,
}

pub fn new_md5_ctx<T>(
	edge_bits: u8,
	proof_size: usize,
	max_sols: u32,
) -> Result<Box<dyn PoWContext<T>>, Error>
where
	T: EdgeType + 'static,
{
	Ok(Box::new(MD5Context {
		phantom: PhantomData::<T> {},
		edge_bits,
		proof_size,
		max_sols,
		header: vec![],
		nonce: 0,
	}))
}

impl<T> PoWContext<T> for MD5Context<T>
where
	T: EdgeType,
{
	fn set_header_nonce(
		&mut self,
		header: Vec<u8>,
		nonce: Option<u64>,
		height: Option<u64>,
		_solve: bool,
	) -> Result<(), Error> {
		self.header = header;
		self.nonce = nonce.unwrap_or(self.nonce);
		Ok(())
	}

	fn pow_solve(&mut self) -> Result<Vec<Proof>, Error> {
		let vector: Vec<u8> = std::iter::repeat(self.edge_bits)
			.take(self.proof_size)
			.chain(self.header.iter().map(|&x| x))
			.collect();
		let digest = md5::compute(vector);
		let proof = format!("{:x}", digest).to_string();
		Ok(vec![Proof::MD5Proof {
			proof,
			edge_bits: self.edge_bits,
		}])
	}

	fn verify(&mut self, proof: &Proof) -> Result<(), Error> {
		let vector: Vec<u8> = std::iter::repeat(self.edge_bits)
			.take(self.proof_size)
			.chain(self.header.iter().map(|&x| x))
			.collect();
		let digest = md5::compute(vector);
		let result = format!("{:x}", digest).to_string();
		if let Proof::MD5Proof { proof: ref p, .. } = proof {
			if result == *p {
				return Ok(());
			}
		}
		return Err(ErrorKind::EdgeAddition)?;
	}
}
