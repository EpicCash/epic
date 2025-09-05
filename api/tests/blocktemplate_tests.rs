use self::chain::types::NoopAdapter;
use self::chain::Chain;
use self::core::genesis;
use self::core::libtx::{self, reward};
use self::core::{consensus, pow};
use epic_chain as chain;
use epic_core as core;
use epic_core::core::Block;
use epic_keychain::{ExtKeychain, Keychain};
use std::fs;
use std::sync::Arc;

#[test]
fn test_external_coinbase_and_set_txhashset_roots() {
	use epic_core::core::{Block, Transaction};
	use epic_core::global;
	use epic_core::global::ChainTypes;
	use epic_core::libtx::ProofBuilder;
	use epic_keychain::{ExtKeychain, Keychain};
	use epic_pool::transaction_pool::TransactionPool;
	use epic_servers::common::adapters::{PoolToChainAdapter, PoolToNetAdapter};
	use epic_servers::common::types::ServerConfig;
	use epic_util::RwLock;
	let chain_dir = ".blocktemplate_test";

	let mut config = ServerConfig::default();
	config.db_root = chain_dir.to_string();

	global::set_mining_mode(ChainTypes::AutomatedTesting);
	let keychain = ExtKeychain::from_random_seed(false).unwrap();
	let genesis = genesis_block(&keychain);
	let chain = init_chain(chain_dir, genesis.clone());

	// Setup a test chain and tx pool (pseudo-code, adapt to your test infra)
	let pool_adapter = Arc::new(PoolToChainAdapter::new());
	let pool_net_adapter = Arc::new(PoolToNetAdapter::new(config.dandelion_config.clone()));
	let _tx_pool = Arc::new(RwLock::new(TransactionPool::new(
		config.pool_config.clone(),
		pool_adapter.clone(),
		pool_net_adapter.clone(),
	)));

	// 1. Get block template from node (without coinbase)
	let head = chain.head_header().unwrap();
	let height = head.height + 1;
	let difficulty = if head.height < consensus::difficultyfix_height() - 1 {
		consensus::next_difficulty(
			head.height + 1,
			(&head.pow.proof).into(),
			chain.difficulty_iter().unwrap(),
		)
	} else {
		consensus::next_difficulty_era1(
			head.height + 1,
			(&head.pow.proof).into(),
			chain.difficulty_iter().unwrap(),
		)
	};

	// 2. Prepare transactions (empty for simplicity)
	let txs: Vec<Transaction> = vec![];

	// 3. EXTERNAL: Build coinbase output and kernel (simulate wallet)
	let keychain = ExtKeychain::from_random_seed(false).unwrap();
	let key_id = ExtKeychain::derive_key_id(0, 1, 0, 0, 0);
	let (coinbase_output, coinbase_kernel) = epic_core::libtx::reward::output(
		&keychain,
		&ProofBuilder::new(&keychain),
		&key_id,
		0, // fees
		false,
		height,
	)
	.unwrap();

	// 4. Node: Build block with txs + coinbase
	let mut block = Block::from_reward(
		&head,
		txs,
		coinbase_output.clone(),
		coinbase_kernel.clone(),
		difficulty.difficulty.clone(),
	)
	.unwrap();

	// 5. Node: Set txhashset roots (must be done after coinbase is added)
	chain.set_txhashset_roots(&mut block).unwrap();

	// 6. Node: Serialize pre_pow for mining
	let pre_pow = {
		let mut header_buf = vec![];
		{
			use epic_core::ser::BinWriter;
			let mut writer = BinWriter::default(&mut header_buf);
			block.header.write_pre_pow(&mut writer).unwrap();
		}
		epic_util::to_hex(header_buf)
	};

	// 7. Assert block is valid and ready for mining
	assert!(block.validate(&head.total_kernel_offset).is_ok());
	assert!(!pre_pow.is_empty());

	// Optionally: Print for debug
	println!("Block ready for mining! pre_pow: {}", pre_pow);
	clean_output_dir(chain_dir);
}

pub fn clean_output_dir(dir_name: &str) {
	let _ = fs::remove_dir_all(dir_name);
}
pub fn init_chain(dir_name: &str, genesis: Block) -> Chain {
	Chain::init(
		dir_name.to_string(),
		Arc::new(NoopAdapter {}),
		genesis,
		pow::verify_size,
		false,
	)
	.unwrap()
}

/// Build genesis block with reward (non-empty, like we have in mainnet).
fn genesis_block<K>(keychain: &K) -> Block
where
	K: Keychain,
{
	let key_id = ExtKeychain::derive_key_id(0, 1, 0, 0, 0);
	let reward = reward::output(
		keychain,
		&libtx::ProofBuilder::new(keychain),
		&key_id,
		0,
		false,
		0,
	)
	.unwrap();

	genesis::genesis_dev().with_reward(reward.0, reward.1)
}
