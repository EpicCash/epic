// Copyright 2026 The Epic Cash Developers
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

//! Syncing of the chain with the rest of the network

mod body_sync;
mod header_sync;
mod state_sync;
mod syncer;

pub use self::syncer::run_sync;

#[cfg(test)]
mod test {
	use crate::chain::{self, types::NoopAdapter};
	use crate::core::core::hash::{Hash, Hashed};
	use crate::core::{genesis, global, pow};
	use crate::core::pow::Difficulty;
	use crate::p2p::{self, Capabilities, Peer, PeerAddr};
	use crate::util::StopState;
	use chrono::Utc;
	use std::net::{SocketAddr, TcpListener};
	use std::path::PathBuf;
	use std::sync::atomic::{AtomicU64, Ordering};
	use std::sync::Arc;
	use std::thread::{self, JoinHandle};
	use std::time::Duration;

	static TEST_ID: AtomicU64 = AtomicU64::new(0);

	pub(super) struct TestNode {
		pub(super) server: Arc<p2p::Server>,
		pub(super) chain: Arc<chain::Chain>,
		pub(super) peer: Arc<Peer>,
		local_peers: Vec<Arc<Peer>>,
		remotes: Vec<TestRemote>,
		local_thread: Option<JoinHandle<()>>,
		root: PathBuf,
	}

	struct TestRemote {
		server: Arc<p2p::Server>,
		peer: Arc<Peer>,
		thread: Option<JoinHandle<()>>,
	}

	impl TestNode {
		pub(super) fn with_outbound_peer(capabilities: Capabilities) -> TestNode {
			global::set_mining_mode(global::ChainTypes::AutomatedTesting);
			let root = std::env::temp_dir().join(format!(
				"epic-sync-test-{}-{}",
				std::process::id(),
				TEST_ID.fetch_add(1, Ordering::Relaxed),
			));
			let genesis = genesis::genesis_dev();
			let genesis_hash = genesis.hash();
			let local_config = config(open_port());
			let server = Arc::new(
				p2p::Server::new(
					&path(&root, "local"),
					Capabilities::UNKNOWN,
					local_config,
					Arc::new(p2p::DummyAdapter {}),
					genesis_hash,
					Arc::new(StopState::new()),
					None,
				)
				.unwrap(),
			);
			let listener = server.clone();
			let local_thread = thread::spawn(move || {
				let _ = listener.listen();
			});
			let (peer, remote) = connect_outbound(
				&root,
				&server,
				capabilities,
				genesis_hash,
				0,
			);
			let chain = Arc::new(
				chain::Chain::init(
					path(&root, "chain"),
					Arc::new(NoopAdapter {}),
					genesis,
					pow::verify_size,
					false,
				)
				.unwrap(),
			);

			TestNode {
				server,
				chain,
				peer: peer.clone(),
				local_peers: vec![peer],
				remotes: vec![remote],
				local_thread: Some(local_thread),
				root,
			}
		}

		pub(super) fn add_outbound_peer(&mut self, capabilities: Capabilities) -> Arc<Peer> {
			let genesis = self.chain.get_header_by_height(0).unwrap().hash();
			let (peer, remote) = connect_outbound(
				&self.root,
				&self.server,
				capabilities,
				genesis,
				self.remotes.len(),
			);
			self.local_peers.push(peer.clone());
			self.remotes.push(remote);
			peer
		}

		pub(super) fn add_inbound_peer(&mut self, capabilities: Capabilities) -> Arc<Peer> {
			let genesis = self.chain.get_header_by_height(0).unwrap().hash();
			let remote_config = config(open_port());
			let remote = Arc::new(
				p2p::Server::new(
					&path(&self.root, &format!("remote-{}", self.remotes.len())),
					capabilities,
					remote_config.clone(),
					Arc::new(p2p::DummyAdapter {}),
					genesis,
					Arc::new(StopState::new()),
					None,
				)
				.unwrap(),
			);
			let local_addr = PeerAddr(SocketAddr::new(
				self.server.config.host,
				self.server.config.port,
			));
			let remote_peer = connect(&remote, local_addr);
			let addr = PeerAddr(SocketAddr::new(remote_config.host, remote_config.port));
			let peer = wait_for_peer(&self.server, addr);
			self.local_peers.push(peer.clone());
			self.remotes.push(TestRemote {
				server: remote,
				peer: remote_peer,
				thread: None,
			});
			peer
		}

		pub(super) fn reconnect_outbound_peer(&mut self) -> Arc<Peer> {
			let addr = self.peer.info.addr;
			self.server.peers.disconnect_peer(addr).unwrap();
			let peer = connect(&self.server, addr);
			self.local_peers.push(peer.clone());
			self.peer = peer.clone();
			peer
		}

		pub(super) fn persist_header_head(&self, height: u64, difficulty: u64) {
			let mut header = self.chain.head_header().unwrap();
			let head = self.chain.header_head().unwrap();
			let header_pmmr = self.chain.header_pmmr();
			let mut header_pmmr = header_pmmr.write();
			let store = self.chain.store();
			let mut batch = store.batch().unwrap();
			chain::txhashset::header_extending(&mut header_pmmr, &head, &mut batch, |ext, batch| {
				for next_height in 1..=height {
					header.prev_hash = header.hash();
					header.height = next_height;
					header.version = crate::core::consensus::header_version(next_height);
					header.pow.total_difficulty =
						Difficulty::from_num(difficulty - height + next_height);
					let mut proof_hash = [0; 32];
					proof_hash[..8].copy_from_slice(&next_height.to_le_bytes());
					header.pow.proof = pow::Proof::RandomXProof { hash: proof_hash };
					batch.save_block_header(&header)?;
					ext.apply_header(&header)?;
				}
				Ok(())
			})
			.unwrap();
			let tip = chain::Tip::from_header(&header);
			batch.save_header_head(&tip).unwrap();
			batch.save_sync_head(&tip).unwrap();
			batch.commit().unwrap();
		}
	}

	impl Drop for TestNode {
		fn drop(&mut self) {
			let now = Utc::now().timestamp();
			for peer in self.local_peers.iter().chain(self.remotes.iter().map(|r| &r.peer)) {
				let _ = peer.send_ping(Difficulty::min(), 0, now);
				peer.stop();
			}
			self.server.stop();
			for remote in &mut self.remotes {
				remote.server.stop();
				if let Some(handle) = remote.thread.take() {
					let _ = handle.join();
				}
			}
			if let Some(handle) = self.local_thread.take() {
				let _ = handle.join();
			}
			let _ = std::fs::remove_dir_all(&self.root);
		}
	}

	fn connect_outbound(
		root: &PathBuf,
		server: &Arc<p2p::Server>,
		capabilities: Capabilities,
		genesis: Hash,
		id: usize,
	) -> (Arc<Peer>, TestRemote) {
		let remote_config = config(open_port());
		let remote = Arc::new(
			p2p::Server::new(
				&path(root, &format!("remote-{}", id)),
				capabilities,
				remote_config.clone(),
				Arc::new(p2p::DummyAdapter {}),
				genesis,
				Arc::new(StopState::new()),
				None,
			)
			.unwrap(),
		);
		let listener = remote.clone();
		let thread = thread::spawn(move || {
			let _ = listener.listen();
		});
		let addr = PeerAddr(SocketAddr::new(remote_config.host, remote_config.port));
		let peer = connect(server, addr);
		let server_addr = PeerAddr(SocketAddr::new(server.config.host, server.config.port));
		let remote_peer = wait_for_peer(&remote, server_addr);
		(
			peer,
			TestRemote {
				server: remote,
				peer: remote_peer,
				thread: Some(thread),
			},
		)
	}

	fn connect(server: &Arc<p2p::Server>, addr: PeerAddr) -> Arc<Peer> {
		(0..50)
			.find_map(|_| match server.connect(addr) {
				Ok(peer) => Some(peer),
				Err(_) => {
					thread::sleep(Duration::from_millis(20));
					None
				}
			})
			.expect("connect test peer")
	}

	fn wait_for_peer(server: &Arc<p2p::Server>, addr: PeerAddr) -> Arc<Peer> {
		(0..50)
			.find_map(|_| {
				let peer = server.peers.get_connected_peer(addr);
				if peer.is_none() {
					thread::sleep(Duration::from_millis(20));
				}
				peer
			})
			.expect("accept test peer")
	}

	fn config(port: u16) -> p2p::P2PConfig {
		p2p::P2PConfig {
			host: "127.0.0.1".parse().unwrap(),
			port,
			peers_allow: None,
			peers_deny: None,
			..p2p::P2PConfig::default()
		}
	}

	fn open_port() -> u16 {
		TcpListener::bind("127.0.0.1:0")
			.unwrap()
			.local_addr()
			.unwrap()
			.port()
	}

	fn path(root: &PathBuf, name: &str) -> String {
		root.join(name).to_string_lossy().into_owned()
	}
}
