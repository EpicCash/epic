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

//! Cuckatoo specific errors
use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
/// Libwallet error types
pub enum Error {
	/// Verification error
	#[error("Verification Error: {0}")]
	Verification(String),
	/// Failure to cast from/to generic integer type
	#[error("IntegerCast")]
	IntegerCast,
	/// IO Error
	#[error("IO Error")]
	IOError,
	/// Unexpected Edge Error
	#[error("Edge Addition Error")]
	EdgeAddition,
	/// Path Error
	#[error("Path Error")]
	Path,
	/// Invalid cycle
	#[error("Invalid Cycle length: {0}")]
	InvalidCycle(usize),
	/// No Cycle
	#[error("No Cycle")]
	NoCycle,
	/// No Solution
	#[error("No Solution")]
	NoSolution,

	#[error("IO error: {0}")]
	Io(#[from] io::Error),
}
