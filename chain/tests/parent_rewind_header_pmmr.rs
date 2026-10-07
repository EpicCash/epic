// Copyright 2026 The Epic Cash Developers
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

#[allow(dead_code)]
mod chain_test_helper;

use chain_test_helper::{
    clean_output_dir, prepare_block, process_block, set_foundation_path_for_test,
};
use epic_chain::{types::NoopAdapter, Chain, Error, Options};
use epic_core::{
    core::{hash::Hashed, Block, BlockHeader},
    pow,
};
use epic_keychain::{ExtKeychain, Keychain};
use std::{fs, sync::Arc};

#[derive(Debug, PartialEq)]
struct HeaderPmmrSnapshot {
    header_head: epic_chain::types::Tip,
    last_pos: u64,
    backend_size: u64,
    data_size: u64,
    hash_file_size: u64,
    data_file_size: u64,
    header_hashes: Vec<Option<epic_core::core::hash::Hash>>,
}

fn reject_pow(_header: &BlockHeader) -> Result<(), pow::Error> {
    Err(pow::Error::Verification(
        "intentional invalid PoW for rejected-header".to_owned(),
    ))
}

fn open_chain(chain_dir: &str, genesis: Block) -> Chain {
    Chain::init(
        chain_dir.to_owned(),
        Arc::new(NoopAdapter {}),
        genesis,
        reject_pow,
        false,
    )
    .unwrap()
}

fn snapshot(chain_dir: &str, chain: &Chain) -> HeaderPmmrSnapshot {
    let header_head = chain.header_head().unwrap();
    let header_pmmr = chain.header_pmmr();
    let header_pmmr = header_pmmr.read();
    let header_hashes = (0..=header_head.height)
        .map(|height| header_pmmr.get_header_hash_by_height(height).ok())
        .collect();
    let pmmr_dir = format!("{chain_dir}/header/header_head");

    HeaderPmmrSnapshot {
        header_head,
        last_pos: header_pmmr.last_pos,
        backend_size: header_pmmr.backend.unpruned_size(),
        data_size: header_pmmr.backend.data_size(),
        hash_file_size: fs::metadata(format!("{pmmr_dir}/pmmr_hash.bin"))
            .unwrap()
            .len(),
        data_file_size: fs::metadata(format!("{pmmr_dir}/pmmr_data.bin"))
            .unwrap()
            .len(),
        header_hashes,
    }
}

#[test]
fn rejected_old_parent_header_cannot_rewind_persistent_header_pmmr() {
    let chain_dir = ".epic.parent_rewind_header_pmmr";
    clean_output_dir(chain_dir);
    set_foundation_path_for_test("foundation_floonet.json");

    let genesis = pow::mine_genesis_block().unwrap();
    let chain = open_chain(chain_dir, genesis.clone());
    let keychain = ExtKeychain::from_random_seed(false).unwrap();

    let mut headers = vec![chain.head_header().unwrap()];
    for n in 1..=5 {
        let block = prepare_block(&keychain, headers.last().unwrap(), &chain, n + 1, vec![], 1);
        headers.push(block.header.clone());
        process_block(&chain, &block);
    }

    let before = snapshot(chain_dir, &chain);
    let old_parent = &headers[1];
    let candidate = prepare_block(&keychain, old_parent, &chain, u64::MAX / 2, vec![], 2);

    // The custom verifier models a structurally decoded peer header that fails
    // full PoW validation. Its declared work is high enough to exercise the
    // pre-validation has_more_work path on vulnerable code.
    let result = chain.process_block_header(&candidate.header, Options::NONE);
    assert!(matches!(result, Err(Error::InvalidPow)));
    assert!(chain.get_block_header(&candidate.hash()).is_err());
    assert_eq!(snapshot(chain_dir, &chain), before);

    drop(chain);
    let reopened = open_chain(chain_dir, genesis);
    assert_eq!(snapshot(chain_dir, &reopened), before);

    drop(reopened);
    clean_output_dir(chain_dir);
}
