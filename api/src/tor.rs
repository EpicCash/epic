// Copyright 2020 The Grin Developers
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

//! Owner API External Definition


use crate::rest::*;
use std::sync::Weak;

/// Main interface into all node API functions.
/// Node APIs are split into two seperate blocks of functionality
/// called the ['Owner'](struct.Owner.html) and ['Foreign'](struct.Foreign.html) APIs
///
/// Methods in this API are intended to be 'single use'.
///

use crate::core::core::transaction::Transaction;
use crate::handlers::pool_api::PoolHandler;
use crate::pool::{self, BlockChain, PoolAdapter};
use crate::util::RwLock;



pub struct Tor<B, P>
where
	B: BlockChain,
	P: PoolAdapter,
{
	pub tx_pool: Weak<RwLock<pool::TransactionPool<B, P>>>,
}

impl<B, P> Tor<B, P>
where
	B: BlockChain,
	P: PoolAdapter,
{
	/// Create a new API instance with the chain, transaction pool, peers and `sync_state`. All subsequent
	/// API calls will operate on this instance of node API.
	///
	/// # Arguments
	/// * `chain` - A non-owning reference of the chain.
	/// * `tx_pool` - A non-owning reference of the transaction pool.
	/// * `peers` - A non-owning reference of the peers.
	/// * `sync_state` - A non-owning reference of the `sync_state`.
	///
	/// # Returns
	/// * An instance of the Node holding references to the current chain, transaction pool, peers and sync_state.
	///

	pub fn new(
		tx_pool: Weak<RwLock<pool::TransactionPool<B, P>>>,
	) -> Self {

		Tor {
			tx_pool,
		}
	}

	/// Push new transaction to our local transaction pool.
	///
	/// # Arguments
	/// * `tx` - the Epic transaction to push.
	/// * `fluff` - boolean to bypass Dandelion relay.
	///
	/// # Returns
	/// * Result Containing:
	/// * `Ok(())` if the transaction was pushed successfully
	/// * or [`Error`](struct.Error.html) if an error is encountered.
	///
	pub fn push_transaction(&self, tx: Transaction, fluff: Option<bool>) -> Result<(), Error> {
		let pool_handler = PoolHandler {
			tx_pool: self.tx_pool.clone(),
		};
		pool_handler.push_transaction(tx, fluff)
	}





}
