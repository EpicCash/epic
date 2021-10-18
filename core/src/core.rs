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

//! Core types

pub mod block;
pub mod block_sums;
pub mod committed;
pub mod compact_block;
pub mod foundation;
pub mod hash;
pub mod id;
pub mod merkle_proof;
pub mod pmmr;
pub mod transaction;

use crate::consensus::EPIC_BASE;

use crate::util::secp::pedersen::Commitment;

pub use self::block::*;
pub use self::block_sums::*;
pub use self::committed::Committed;
pub use self::compact_block::*;
pub use self::id::ShortId;
pub use self::transaction::*;

/// Common errors
#[derive(Fail, Debug)]
pub enum Error {
	/// Human readable represenation of amount is invalid
	#[fail(display = "Amount string was invalid")]
	InvalidAmountString,
}

/// Common method for parsing an amount from human-readable, and converting
/// to internally-compatible u64

pub fn amount_from_hr_string(amount: &str) -> Result<u64, Error> {
	// no i18n yet, make sure we use '.' as the separator
	if amount.find(',').is_some() {
		return Err(Error::InvalidAmountString);
	}
	let (epics, nepics) = match amount.find('.') {
		None => (parse_epics(amount)?, 0),
		Some(pos) => {
			let (gs, tail) = amount.split_at(pos);
			(parse_epics(gs)?, parse_nepics(&tail[1..])?)
		}
	};
	Ok(epics * EPIC_BASE + nepics)
}

fn parse_epics(amount: &str) -> Result<u64, Error> {
	if amount == "" {
		Ok(0)
	} else {
		amount
			.parse::<u64>()
			.map_err(|_| Error::InvalidAmountString)
	}
}

lazy_static! {
	static ref WIDTH: usize = (EPIC_BASE as f64).log(10.0) as usize;
}

fn parse_nepics(amount: &str) -> Result<u64, Error> {
	let amount = if amount.len() > *WIDTH {
		&amount[..*WIDTH]
	} else {
		amount
	};
	format!("{:0<width$}", amount, width = WIDTH)
		.parse::<u64>()
		.map_err(|_| Error::InvalidAmountString)
}

/// Common method for converting an amount to a human-readable string

pub fn amount_to_hr_string(amount: u64, truncate: bool) -> String {
	let amount = (amount as f64 / EPIC_BASE as f64) as f64;
	let hr = format!("{:.*}", WIDTH, amount);
	if truncate {
		let nzeros = hr.chars().rev().take_while(|x| x == &'0').count();
		if nzeros < *WIDTH {
			return hr.trim_end_matches('0').to_string();
		} else {
			return format!("{}0", hr.trim_end_matches('0'));
		}
	}
	hr
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	pub fn test_amount_from_hr() {
		assert!(5012345678 == amount_from_hr_string("50.123456789").unwrap());
		assert!(5012345678 == amount_from_hr_string("50.1234567899").unwrap());
		assert!(50 == amount_from_hr_string(".00000050").unwrap());
		assert!(1 == amount_from_hr_string(".00000001").unwrap());
		assert!(0 == amount_from_hr_string(".000000009").unwrap());
		assert!(50_000_000_000 == amount_from_hr_string("500").unwrap());
		assert!(
			500_000_000_000_000_000 == amount_from_hr_string("5000000000.00000000000").unwrap()
		);
		assert!(6_660_000_000 == amount_from_hr_string("66.6").unwrap());
		assert!(6_600_000_000 == amount_from_hr_string("66.").unwrap());
	}

	#[test]
	pub fn test_amount_to_hr() {
		assert!("50.12345678" == amount_to_hr_string(5012345678, false));
		assert!("50.12345678" == amount_to_hr_string(5012345678, true));
		assert!("0.00000050" == amount_to_hr_string(50, false));
		assert!("0.0000005" == amount_to_hr_string(50, true));
		assert!("0.00000001" == amount_to_hr_string(1, false));
		assert!("0.00000001" == amount_to_hr_string(1, true));
		assert!("500.00000000" == amount_to_hr_string(50_000_000_000, false));
		assert!("500.0" == amount_to_hr_string(50_000_000_000, true));
		assert!("5000000000.00000000" == amount_to_hr_string(500_000_000_000_000_000, false));
		assert!("5000000000.0" == amount_to_hr_string(500_000_000_000_000_000, true));
		assert!("66.6" == amount_to_hr_string(6660000000, true));
	}
}
