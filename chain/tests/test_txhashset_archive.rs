// Copyright 2019 The Grin Developers
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

mod chain_test_helper;
use epic_core as core;

use crate::chain_test_helper::{clean_output_dir, init_chain, prepare_block, process_block};
use epic_keychain as keychain;

use self::core::global::ChainTypes;
use self::core::{global, pow};
use self::keychain::{ExtKeychain, Keychain};
#[test]
fn test_txhashset_archive_header() {
	let chain_dir = ".txhashset_archive_test";
	clean_output_dir(chain_dir);

	global::set_mining_mode(ChainTypes::AutomatedTesting);
	let project_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let foundation_path = format!("{}/../debian/foundation_floonet.json", project_dir);
	global::set_foundation_path(foundation_path);

	let genesis = pow::mine_genesis_block().unwrap();
	let chain = init_chain(chain_dir, genesis);

	let kc = ExtKeychain::from_random_seed(false).unwrap();
	let mut prev = chain.head_header().unwrap();

	// Mine 35 blocks after genesis
	for n in 1..=35 {
		let b = prepare_block(&kc, &prev, &chain, n + 1, vec![], 1);
		prev = b.header.clone();
		process_block(&chain, &b);
	}

	let header = chain.txhashset_archive_header().unwrap();
	assert_eq!(header.height, 10);

	clean_output_dir(chain_dir);
}
