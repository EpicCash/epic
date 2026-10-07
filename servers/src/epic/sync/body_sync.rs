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

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::prelude::{DateTime, Utc};
use chrono::Duration;

use crate::chain::{self, SyncState, SyncStatus};
use crate::core::core::hash::Hash;
use crate::p2p;
use epic_p2p::types::MAX_BLOCK_BODIES;
use epic_p2p::PeerAddr;

pub struct BodySync {
	chain: Arc<chain::Chain>,
	peers: Arc<p2p::Peers>,
	sync_state: Arc<SyncState>,
	pending_requests: HashMap<Hash, (PeerAddr, DateTime<Utc>)>,
	hashes_to_get: Vec<Hash>,
}

fn request_window(peer_count: usize, orphan_count: usize) -> usize {
	let window = peer_count
		.min(MAX_BLOCK_BODIES as usize)
		.min(chain::MAX_ORPHAN_SIZE.saturating_sub(orphan_count));
	if peer_count == 0 {
		0
	} else {
		window.max(1)
	}
}

impl BodySync {
	pub fn new(
		sync_state: Arc<SyncState>,
		peers: Arc<p2p::Peers>,
		chain: Arc<chain::Chain>,
	) -> BodySync {
		BodySync {
			sync_state,
			peers,
			chain,
			pending_requests: HashMap::new(),
			hashes_to_get: Vec::new(),
		}
	}

	/// Check whether a body sync is needed and run it if so.
	/// Return true if txhashset download is needed (when requested block is under the horizon).
	pub fn check_run(
		&mut self,
		_head: &chain::Tip,
		_highest_height: u64,
	) -> Result<bool, chain::Error> {
		self.cleanup_completed_requests()?;
		self.cleanup_stale_block_requests();
		let peers = self.peers.outgoing_connected_peers();
		self.cleanup_disconnected_peers(&peers);

		match self.sync_state.status() {
			SyncStatus::TxHashsetSetup
			| SyncStatus::TxHashsetKernelsValidation { .. }
			| SyncStatus::TxHashsetRangeProofsValidation { .. } => {
				return Ok(false);
			}
			_ => {}
		}

		if self.body_sync_due(&peers) {
			if self.body_sync(&peers)? {
				return Ok(true);
			}
		}
		Ok(false)
	}

	fn body_sync(&mut self, peers: &[Arc<p2p::Peer>]) -> Result<bool, chain::Error> {
		if peers.is_empty() {
			debug!("body_sync: no peers, nothing to do");
			return Ok(false);
		}

		// If no new hashes are available, fetch new ones
		if self.hashes_to_get.is_empty() {
			if self.fetch_new_hashes()? {
				return Ok(true); // TxHashset download required
			}
		}

		// Filtere Hashes, um nur die noch nicht verarbeiteten zu behalten
		self.filter_unprocessed_hashes()?;

		if self.hashes_to_get.is_empty() {
			debug!("body_sync: no new hashes to request");
			return Ok(false);
		}

		// Send requests to available peers
		let requested = self.request_blocks_from_peers(peers)?;

		self.log_sync_progress(requested)?;

		Ok(false)
	}

	// Should we run block body sync and ask for more full blocks?
	fn body_sync_due(&self, peers: &[Arc<p2p::Peer>]) -> bool {
		if self.hashes_to_get.is_empty() {
			return true;
		}

		let window = request_window(peers.len(), self.chain.orphans_len());
		self.pending_requests.len() < window
			&& peers.iter().any(|peer| {
				self.pending_requests
					.values()
					.all(|(addr, _)| addr != &peer.info.addr)
			})
	}

	fn cleanup_completed_requests(&mut self) -> Result<usize, chain::Error> {
		let mut to_remove = vec![];

		for hash in self.pending_requests.keys() {
			if self.chain.block_exists(*hash)? || self.chain.is_orphan(hash) {
				to_remove.push(*hash);
			}
		}

		let completed = to_remove.len();
		for hash in to_remove {
			self.pending_requests.remove(&hash);
		}

		Ok(completed)
	}

	fn cleanup_stale_block_requests(&mut self) {
		let now = Utc::now();
		let timeout = Duration::seconds(10);
		let expired = self
			.pending_requests
			.iter()
			.filter(|(_, (_, timestamp))| now.signed_duration_since(*timestamp) > timeout)
			.map(|(hash, (peer_addr, _))| (*hash, *peer_addr))
			.collect::<Vec<_>>();
		for (hash, peer_addr) in expired {
			if let Err(e) = self.peers.disconnect_peer(peer_addr) {
				warn!("Failed to disconnect peer {}: {:?}", peer_addr, e);
			} else {
				info!("Disconnected peer {} due to block request timeout", peer_addr);
			}
			self.pending_requests.remove(&hash);
			warn!(
				"Block request for {:?} from peer {} timed out, will retry with another peer.",
				hash, peer_addr
			);
		}
	}

	fn cleanup_disconnected_peers(&mut self, peers: &[Arc<p2p::Peer>]) {
		let connected = peers.iter().map(|peer| peer.info.addr).collect::<HashSet<_>>();
		self.pending_requests
			.retain(|_, (addr, _)| connected.contains(addr));
	}

	fn fetch_new_hashes(&mut self) -> Result<bool, chain::Error> {
		let mut hashes: Option<Vec<Hash>> = Some(vec![]);
		let txhashset_needed = match self
			.chain
			.check_txhashset_needed("body_sync".to_owned(), &mut hashes)
		{
			Ok(v) => v,
			Err(e) => {
				error!("body_sync: failed to call txhashset_needed: {:?}", e);
				return Ok(false);
			}
		};

		if txhashset_needed {
			info!("Block synchronization is out of range. Starting txhashset download.");
			return Ok(true);
		}

		self.hashes_to_get = match hashes {
			Some(v) => v,
			None => {
				error!("unexpected: hashes is None");
				return Ok(false);
			}
		};

		self.hashes_to_get.reverse();
		Ok(false)
	}

	fn filter_unprocessed_hashes(&mut self) -> Result<(), chain::Error> {
		self.hashes_to_get = self
			.hashes_to_get
			.drain(..)
			.filter(|x| !self.chain.get_block(x).is_ok() && !self.chain.is_orphan(x))
			.collect();
		Ok(())
	}

	fn request_blocks_from_peers(
		&mut self,
		peers: &[Arc<p2p::Peer>],
	) -> Result<usize, chain::Error> {
		let window = request_window(peers.len(), self.chain.orphans_len());
		let open_slots = window.saturating_sub(self.pending_requests.len());
		if open_slots == 0 {
			return Ok(0);
		}

		let pending_peers = self
			.pending_requests
			.values()
			.map(|(addr, _)| *addr)
			.collect::<HashSet<_>>();
		let free_peers = peers
			.iter()
			.filter(|peer| !pending_peers.contains(&peer.info.addr))
			.take(open_slots)
			.collect::<Vec<_>>();
		let hashes = self
			.hashes_to_get
			.iter()
			.filter(|hash| !self.pending_requests.contains_key(hash))
			.take(free_peers.len())
			.copied()
			.collect::<Vec<_>>();

		let mut requested = 0;
		for (peer, hash) in free_peers.into_iter().zip(hashes) {
			if let Err(e) = peer.send_block_request(hash, chain::Options::SYNC) {
				debug!("Skipped request to {}: {:?}", peer.info.addr, e);
				peer.stop();
			} else {
				debug!("Requested block {:?} from peer {:?}", hash, peer.info.addr);
				self.pending_requests
					.insert(hash, (peer.info.addr, Utc::now()));
				requested += 1;
			}
		}
		Ok(requested)
	}

	fn log_sync_progress(&self, requested: usize) -> Result<(), chain::Error> {
		if requested > 0 {
			let body_head = self.chain.head()?;
			let header_head = self.chain.header_head()?;

			let remaining_blocks = header_head.height - body_head.height;
			let total_blocks = header_head.height;
			let percentage_synced =
				(((total_blocks - remaining_blocks) as f64 / total_blocks as f64) * 10_000.0)
					.trunc() / 100.0;

			let max_width = remaining_blocks
				.to_string()
				.len()
				.max(self.hashes_to_get.len().to_string().len());

			info!(
				"Block Sync: Requested {:>width$} more block(s), {:>width$} block(s) remaining, {:>6.2}% completed",
				requested,
				remaining_blocks,
				percentage_synced,
				width = max_width
			);
		}
		Ok(())
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::core::core::hash::Hashed;
	use crate::epic::sync::test::TestNode;
	use crate::p2p::Capabilities;

	#[test]
	fn request_window_respects_protocol_and_orphan_caps() {
		assert_eq!(request_window(32, 0), MAX_BLOCK_BODIES as usize);
		assert_eq!(request_window(32, chain::MAX_ORPHAN_SIZE - 3), 3);
		assert_eq!(request_window(32, chain::MAX_ORPHAN_SIZE), 1);
		assert_eq!(request_window(0, chain::MAX_ORPHAN_SIZE), 0);
	}

	#[test]
	fn parent_block_does_not_complete_requested_block() {
		let node = TestNode::with_outbound_peer(Capabilities::UNKNOWN);
		node.persist_header_head(1, 100);
		let hash = node.chain.get_header_by_height(1).unwrap().hash();
		let mut sync = BodySync::new(
			Arc::new(SyncState::new()),
			node.server.peers.clone(),
			node.chain.clone(),
		);
		sync.pending_requests
			.insert(hash, (node.peer.info.addr, Utc::now()));

		assert_eq!(sync.cleanup_completed_requests().unwrap(), 0);
		assert!(sync.pending_requests.contains_key(&hash));
	}

	#[test]
	fn exact_block_completes_requested_block() {
		let node = TestNode::with_outbound_peer(Capabilities::UNKNOWN);
		let hash = node.chain.head().unwrap().hash();
		let mut sync = BodySync::new(
			Arc::new(SyncState::new()),
			node.server.peers.clone(),
			node.chain.clone(),
		);
		sync.pending_requests
			.insert(hash, (node.peer.info.addr, Utc::now()));

		assert_eq!(sync.cleanup_completed_requests().unwrap(), 1);
		assert!(sync.pending_requests.is_empty());
	}

	#[test]
	fn block_sync_requests_one_block_from_each_available_peer() {
		let mut node = TestNode::with_outbound_peer(Capabilities::UNKNOWN);
		node.add_outbound_peer(Capabilities::UNKNOWN);
		node.add_outbound_peer(Capabilities::UNKNOWN);
		node.persist_header_head(4, 100);
		let mut sync = BodySync::new(
			Arc::new(SyncState::new()),
			node.server.peers.clone(),
			node.chain.clone(),
		);
		sync.hashes_to_get = (1..=4)
			.map(|height| node.chain.get_header_by_height(height).unwrap().hash())
			.collect();

		let peers = node.server.peers.outgoing_connected_peers();
		sync.request_blocks_from_peers(&peers).unwrap();

		assert_eq!(sync.pending_requests.len(), 3);
		assert_eq!(
			sync
				.pending_requests
				.values()
				.map(|(addr, _)| *addr)
				.collect::<std::collections::HashSet<_>>()
				.len(),
			3
		);
	}

	#[test]
	fn completed_request_refills_its_peer_slot() {
		let mut node = TestNode::with_outbound_peer(Capabilities::UNKNOWN);
		let second_peer = node.add_outbound_peer(Capabilities::UNKNOWN);
		node.persist_header_head(2, 100);
		let genesis = node.chain.head().unwrap().hash();
		let first = node.chain.get_header_by_height(1).unwrap().hash();
		let second = node.chain.get_header_by_height(2).unwrap().hash();
		let mut sync = BodySync::new(
			Arc::new(SyncState::new()),
			node.server.peers.clone(),
			node.chain.clone(),
		);
		sync.hashes_to_get = vec![genesis, first, second];
		sync
			.pending_requests
			.insert(genesis, (node.peer.info.addr, Utc::now()));
		sync
			.pending_requests
			.insert(first, (second_peer.info.addr, Utc::now()));

		assert_eq!(sync.cleanup_completed_requests().unwrap(), 1);
		sync.filter_unprocessed_hashes().unwrap();
		let peers = node.server.peers.outgoing_connected_peers();
		assert!(sync.body_sync_due(&peers));
		assert_eq!(sync.request_blocks_from_peers(&peers).unwrap(), 1);
		assert_eq!(sync.pending_requests.len(), 2);
		assert!(sync
			.pending_requests
			.get(&second)
			.map(|(addr, _)| *addr == node.peer.info.addr)
			.unwrap_or(false));
	}

	#[test]
	fn timed_out_request_disconnects_without_banning() {
		let node = TestNode::with_outbound_peer(Capabilities::UNKNOWN);
		node.persist_header_head(1, 100);
		let hash = node.chain.get_header_by_height(1).unwrap().hash();
		let mut sync = BodySync::new(
			Arc::new(SyncState::new()),
			node.server.peers.clone(),
			node.chain.clone(),
		);
		sync
			.pending_requests
			.insert(hash, (node.peer.info.addr, Utc::now() - Duration::seconds(11)));

		sync.cleanup_stale_block_requests();

		assert!(sync.pending_requests.is_empty());
		assert!(!node.peer.is_banned());
		assert!(!node.server.peers.is_banned(node.peer.info.addr));
	}
}
