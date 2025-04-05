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
		head: &chain::Tip,
		highest_height: u64,
	) -> Result<bool, chain::Error> {
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

			self.sync_state.update(SyncStatus::BodySync {
				current_height: head.height,
				highest_height,
			});
		}
		Ok(false)
	}

	fn body_sync(&mut self) -> Result<bool, chain::Error> {
		let peers = self.peers.more_work_peers()?;
		if peers.is_empty() {
			debug!("body_sync: no peers, nothing to do");
			thread::sleep(time::Duration::from_secs(10));
			return Ok(false);
		}

		// Fetch new hashes if the current list is empty
		if self.hashes_to_get.is_empty() {
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
		}

		// Filter hashes to get only those not yet processed
		self.hashes_to_get = self
			.hashes_to_get
			.drain(..)
			.filter(|x| !self.chain.get_block(x).is_ok() && !self.chain.is_orphan(x))
			.collect();

		if self.hashes_to_get.is_empty() {
			debug!("body_sync: no new hashes to request");
			return Ok(false);
		}

		let body_head = self.chain.head()?;
		let header_head = self.chain.header_head()?;

		let remaining_blocks = header_head.height - body_head.height;
		let total_blocks = header_head.height;
		let percentage_synced = (((total_blocks - remaining_blocks) as f64 / total_blocks as f64)
			* 10_000.0)
			.trunc() / 100.0;

		let max_width = remaining_blocks
			.to_string()
			.len()
			.max(self.hashes_to_get.len().to_string().len());

		// Reinitialize download tracking state
		self.blocks_requested = 0;

		let mut peers_iter = peers.iter();
		for hash in self.hashes_to_get.clone() {
			// Überprüfe, ob der Hash bereits angefordert wurde und ob der Timeout überschritten ist
			let should_request = match self.hash_request_timestamps.get(&hash) {
				Some(timestamp) => Utc::now() > *timestamp + Duration::seconds(5), // 30 Sekunden Timeout
				None => true, // Hash wurde noch nicht angefordert
			};

			if !should_request {
				debug!("Hash {:?} is already requested recently, skipping.", hash);
				continue;
			}

			if let Some(peer) = peers_iter.find(|peer| {
				!self
					.requested_peers
					.iter()
					.any(|(addr, _)| addr == &peer.info.addr)
			}) {
				if let Err(e) = peer.send_block_request(hash, chain::Options::SYNC) {
					debug!("Skipped request to {}: {:?}", peer.info.addr, e);
					peer.stop();
				} else {
					self.blocks_requested += 1;
					self.requested_peers.insert((peer.info.addr.clone(), hash));
					self.hash_request_timestamps.insert(hash, Utc::now()); // Aktualisiere den Zeitstempel
				}
			} else {
				//debug!("No available peers to request hash {}", hash);
				continue;
			}
		}

		if self.blocks_requested > 0 {
			self.receive_timeout = Utc::now() + Duration::seconds(120);
			info!(
				"Block Sync: Requested {:>width$} more block(s), {:>width$} block(s) remaining, {:>6.2}% completed",
				self.blocks_requested,
				remaining_blocks,
				percentage_synced,
				width = max_width
			);
		}

		Ok(false)
	}

	// Should we run block body sync and ask for more full blocks?
	fn body_sync_due(&mut self) -> Result<bool, chain::Error> {
		let blocks_received = self.blocks_received()?;

		// Wenn Blöcke angefordert wurden, aber keine empfangen wurden, starte neu
		if self.blocks_requested > 0 {
			let timeout = Utc::now() > self.receive_timeout;
			if timeout && blocks_received <= self.prev_blocks_received {
				warn!(
					"Block Sync: expecting {} more blocks and none received for a while. Resetting state.",
					self.blocks_requested,
				);

				// Listen zurücksetzen
				self.hashes_to_get.clear();
				self.requested_peers.clear();
				self.hash_request_timestamps.clear();
				self.blocks_requested = 0;
				self.prev_blocks_received = 0;

				// Synchronisierung erneut starten
				return Ok(true);
			}
		}

		// Aktualisiere den Status, wenn Blöcke empfangen wurden
		if blocks_received > self.prev_blocks_received {
			self.blocks_requested = self
				.blocks_requested
				.saturating_sub(blocks_received - self.prev_blocks_received);
			self.prev_blocks_received = blocks_received;
		}

		// Überprüfe, ob ein Peer verfügbar ist, um neue Anfragen zu senden
		if self.peers.more_work_peers()?.iter().any(|peer| {
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
		let blocks_received = (self.chain.head()?).height
			+ self.chain.orphans_len() as u64
			+ self.chain.orphans_evicted_len() as u64;

		// Remove peers from requested_peers when a block is received
		let mut received_hashes = vec![];
		for (peer_addr, hash) in &self.requested_peers {
			if self.chain.get_block(hash).is_ok() {
				received_hashes.push((*peer_addr, *hash));
			}
		}

		for (peer_addr, hash) in received_hashes {
			self.requested_peers.remove(&(peer_addr, hash));
			self.hash_request_timestamps.remove(&hash); // Entferne den Zeitstempel
		}

		Ok(blocks_received)
	}
}
