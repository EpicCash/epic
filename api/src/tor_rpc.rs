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

//! JSON-RPC Stub generation for the Owner API

use crate::tor::Tor;
use crate::core::core::transaction::Transaction;
use crate::pool::{BlockChain, PoolAdapter};
use crate::rest::Error;

/// Public definition used to generate Node jsonrpc api.
/// * When running `epic` with defaults, the V2 api is available at
/// `localhost:3413/v2/tor`
/// * The endpoint only supports POST operations, with the json-rpc request as the body
#[easy_jsonrpc_mw::rpc]
pub trait TorRpc: Sync + Send {

    fn push_transaction(&self, tx: Transaction, fluff: Option<bool>) -> Result<(), Error>;

}

impl<B, P> TorRpc for Tor<B, P>
where
	B: BlockChain,
	P: PoolAdapter,
{
    fn push_transaction(&self, tx: Transaction, fluff: Option<bool>) -> Result<(), Error> {
		Tor::push_transaction(self, tx, fluff)
	}
}