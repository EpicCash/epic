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

use std::sync::Arc;
use std::thread;
use std::time;

use chrono::prelude::{DateTime, Utc};
use chrono::Duration;

use crate::chain::{self, SyncState, SyncStatus};
use crate::core::core::hash::Hash;
use crate::p2p;
use epic_p2p::PeerAddr;

pub struct BodySync {
	chain: Arc<chain::Chain>,
	peers: Arc<p2p::Peers>,
	sync_state: Arc<SyncState>,
	blocks_requested: u64,
	receive_timeout: DateTime<Utc>,
	prev_blocks_received: u64,
	requested_peers: std::collections::HashSet<(PeerAddr, Hash)>,
	hashes_to_get: Vec<Hash>,
	hash_request_timestamps: std::collections::HashMap<Hash, DateTime<Utc>>, // Zeitstempel für Hash-Anfragen
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
			blocks_requested: 0,
			receive_timeout: Utc::now(),
			prev_blocks_received: 0,
			requested_peers: std::collections::HashSet::new(),
			hashes_to_get: Vec::new(),
			hash_request_timestamps: std::collections::HashMap::new(), // Initialisiere als leer
		}
	}

	/// Check whether a body sync is needed and run it if so.
	/// Return true if txhashset download is needed (when requested block is under the horizon).
	pub fn check_run(
		&mut self,
		_head: &chain::Tip,
		_highest_height: u64,
	) -> Result<bool, chain::Error> {
		self.cleanup_disconnected_peers();

		match self.sync_state.status() {
			SyncStatus::TxHashsetSetup
			| SyncStatus::TxHashsetKernelsValidation { .. }
			| SyncStatus::TxHashsetRangeProofsValidation { .. } => {
				return Ok(false);
			}
			_ => {}
		}

		if self.body_sync_due()? {
			if self.body_sync()? {
				return Ok(true);
			}
		}
		Ok(false)
	}

	fn body_sync(&mut self) -> Result<bool, chain::Error> {
		self.cleanup_stale_block_requests();
		let peers = self.peers.outgoing_connected_peers();
		if peers.is_empty() {
			debug!("body_sync: no peers, nothing to do");
			thread::sleep(time::Duration::from_secs(10));
			return Ok(false);
		}

		// Check if new blocks have been received and update the status
		self.update_blocks_received()?;

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

		// Initialize progress
		self.blocks_requested = 0;

		// Send requests to available peers
		self.request_blocks_from_peers()?;

		// Wait for blocks to be received or timeout
		self.wait_for_blocks()?;

		// Update timeout and log progress
		self.log_sync_progress()?;

		Ok(false)
	}

	// Should we run block body sync and ask for more full blocks?
	fn body_sync_due(&mut self) -> Result<bool, chain::Error> {
		let blocks_received = self.blocks_received()?;

		// If blocks were requested but none were received, reset state
		if self.blocks_requested > 0 {
			let timeout = Utc::now() > self.receive_timeout;
			if timeout && blocks_received <= self.prev_blocks_received {
				warn!(
                    "Block Sync: expecting {} more blocks and none received for a while. Resetting state.",
                    self.blocks_requested,
                );

				// Reset lists
				self.hashes_to_get.clear();
				self.requested_peers.clear();
				self.hash_request_timestamps.clear();
				self.blocks_requested = 0;
				self.prev_blocks_received = 0;
				self.receive_timeout = Utc::now(); // Reset timeout

				// Restart synchronization
				return Ok(false);
			}
		}

		// Update status if blocks were received
		if blocks_received > self.prev_blocks_received {
			self.blocks_requested = self
				.blocks_requested
				.saturating_sub(blocks_received - self.prev_blocks_received);
			self.prev_blocks_received = blocks_received;
		}

		// Check if a peer is available to send new requests
		if self.peers.outgoing_connected_peers().iter().any(|peer| {
			self.requested_peers
				.iter()
				.all(|(addr, _)| addr != &peer.info.addr)
		}) {
			return Ok(true);
		}

		Ok(false)
	}

	// Total numbers received on this chain, including the head and orphans
	fn blocks_received(&mut self) -> Result<u64, chain::Error> {
		let mut received = 0;
		let mut to_remove = vec![];

		for (peer_addr, hash) in self.requested_peers.iter() {
			if let Ok(header) = self.chain.get_block_header(hash) {
				// Check if the parent block exists
				if self.chain.get_block(&header.prev_hash).is_ok() {
					to_remove.push((peer_addr.clone(), *hash));
				}
			}
		}

		for (peer_addr, hash) in to_remove {
			self.requested_peers.remove(&(peer_addr, hash));
			self.hash_request_timestamps.remove(&hash);
			self.receive_timeout = Utc::now() + Duration::seconds(20);
			received += 1;
		}

		Ok(received)
	}

	fn update_blocks_received(&mut self) -> Result<(), chain::Error> {
		let blocks_received = self.blocks_received()?;
		if blocks_received > self.prev_blocks_received {
			self.blocks_requested = self
				.blocks_requested
				.saturating_sub(blocks_received - self.prev_blocks_received);
			self.prev_blocks_received = blocks_received;
		}
		Ok(())
	}

	fn cleanup_stale_block_requests(&mut self) {
		let now = Utc::now();
		let timeout = Duration::seconds(20);
		let mut to_remove = vec![];
		for (peer_addr, hash) in self.requested_peers.iter() {
			if let Some(ts) = self.hash_request_timestamps.get(hash) {
				if now.signed_duration_since(*ts) > timeout {
					to_remove.push((peer_addr.clone(), *hash));
				}
			}
		}
		for (peer_addr, hash) in to_remove {
			self.requested_peers.remove(&(peer_addr, hash));
			self.hash_request_timestamps.remove(&hash);
			warn!(
				"Block request for {:?} from peer {} timed out, will retry with another peer.",
				hash, peer_addr
			);
		}
	}

	fn cleanup_disconnected_peers(&mut self) {
		let connected: std::collections::HashSet<_> = self
			.peers
			.outgoing_connected_peers()
			.iter()
			.map(|p| p.info.addr)
			.collect();
		let to_remove: Vec<_> = self
			.requested_peers
			.iter()
			.filter(|(addr, _)| !connected.contains(addr))
			.cloned()
			.collect();
		for (addr, hash) in to_remove {
			self.requested_peers.remove(&(addr, hash));
			self.hash_request_timestamps.remove(&hash);
		}
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

	fn request_blocks_from_peers(&mut self) -> Result<(), chain::Error> {
		let peers = self.peers.outgoing_connected_peers();

		// Number of blocks to request in parallel
		// only 1 works best, maybe if peers do not block
		// when they receive a header we can higher this value
		let max_parallel = 1;

		for hash in self.hashes_to_get.iter().take(max_parallel) {
			let should_request = match self.hash_request_timestamps.get(hash) {
				Some(timestamp) => Utc::now() > *timestamp + Duration::seconds(5),
				None => true,
			};

			if !should_request {
				continue;
			}

			// Find a peer that hasn't been asked for this hash
			if let Some(peer) = peers.iter().find(|peer| {
				!self
					.requested_peers
					.iter()
					.any(|(addr, h)| addr == &peer.info.addr && h == hash)
			}) {
				if let Err(e) = peer.send_block_request(*hash, chain::Options::SYNC) {
					debug!("Skipped request to {}: {:?}", peer.info.addr, e);
					peer.stop();
				} else {
					debug!("Requested block {:?} from peer {:?}", hash, peer.info.addr);
					self.blocks_requested += 1;
					self.requested_peers.insert((peer.info.addr.clone(), *hash));
					self.hash_request_timestamps.insert(*hash, Utc::now());
				}
			}
		}
		Ok(())
	}

	fn log_sync_progress(&mut self) -> Result<(), chain::Error> {
		if self.blocks_requested > 0 {
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
				self.blocks_requested,
				remaining_blocks,
				percentage_synced,
				width = max_width
			);
		}
		Ok(())
	}

	fn wait_for_blocks(&mut self) -> Result<(), chain::Error> {
		let start_time = Utc::now();
		while Utc::now() < start_time + Duration::seconds(60) {
			let blocks_received = self.blocks_received()?;
			if blocks_received > 0 {
				debug!("Block received, proceeding to the next block.");
				self.hashes_to_get.remove(0);
				break;
			}
			thread::sleep(time::Duration::from_millis(100));
		}
		Ok(())
	}
}
