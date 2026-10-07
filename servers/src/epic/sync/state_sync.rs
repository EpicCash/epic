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

use chrono::prelude::{DateTime, Utc};
use chrono::Duration;
use std::sync::Arc;

use crate::chain::{self, SyncState, SyncStatus};
use crate::core::core::hash::Hashed;
use crate::core::global;
use crate::p2p::{self, Peer};

const TXHASHSET_ARCHIVE_BASE_BYTES: u64 = 64 * 1024 * 1024;
const TXHASHSET_ARCHIVE_BYTES_PER_OUTPUT_POS: u64 = 1024;
const TXHASHSET_ARCHIVE_BYTES_PER_KERNEL_POS: u64 = 512;
const TXHASHSET_ARCHIVE_HARD_MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;

fn txhashset_archive_size_limit(header: &crate::core::core::BlockHeader) -> u64 {
	TXHASHSET_ARCHIVE_BASE_BYTES
		.saturating_add(
			header
				.output_mmr_size
				.saturating_mul(TXHASHSET_ARCHIVE_BYTES_PER_OUTPUT_POS),
		)
		.saturating_add(
			header
				.kernel_mmr_size
				.saturating_mul(TXHASHSET_ARCHIVE_BYTES_PER_KERNEL_POS),
		)
		.min(TXHASHSET_ARCHIVE_HARD_MAX_BYTES)
}

/// Fast sync has 3 "states":
/// * syncing headers
/// * once all headers are sync'd, requesting the txhashset state if its over horizon of 2880 blocks (2 day heights)
/// * once we have the state, get blocks after that
///
/// The StateSync struct implements and monitors the middle step.
pub struct StateSync {
	sync_state: Arc<SyncState>,
	peers: Arc<p2p::Peers>,
	chain: Arc<chain::Chain>,

	prev_state_sync: Option<DateTime<Utc>>,
	state_sync_peer: Option<Arc<Peer>>,
	state_sync_retry: bool,
}

impl StateSync {
	pub fn new(
		sync_state: Arc<SyncState>,
		peers: Arc<p2p::Peers>,
		chain: Arc<chain::Chain>,
	) -> StateSync {
		StateSync {
			sync_state,
			peers,
			chain,
			prev_state_sync: None,
			state_sync_peer: None,
			state_sync_retry: false,
		}
	}

	/// Check whether state sync should run and triggers a state download when
	/// it's time (we have all headers). Returns true as long as state sync
	/// needs monitoring, false when it's either done or turned off.
	pub fn check_run(
		&mut self,
		header_head: &chain::Tip,
		head: &chain::Tip,
		tail: &chain::Tip,
		highest_height: u64,
	) -> bool {
		trace!(
			"head.height: {}, tail.height: {}. header_head.height: {}, highest_height: {}",
			head.height,
			tail.height,
			header_head.height,
			highest_height,
		);

		// check sync error
		let sync_error = self.sync_state.sync_error();
		let has_sync_error = {
			let sync_error = sync_error.read();
			if let Some(ref error) = *sync_error {
				error!("Error = {:?}. restart txhashset sync", error);
				true
			} else {
				false
			}
		};
		if has_sync_error {
			self.schedule_retry();
			self.sync_state.clear_sync_error();
		}

		match self.sync_state.status() {
			SyncStatus::TxHashsetDone => return false,
			SyncStatus::TxHashsetSetup
			| SyncStatus::TxHashsetKernelsValidation { .. }
			| SyncStatus::TxHashsetRangeProofsValidation { .. }
				if !self.state_sync_retry =>
			{
				return false;
			}
			_ => {}
		}

		// check peer connection status of this sync
		if let SyncStatus::TxHashsetDownload { .. } = self.sync_state.status() {
			if let Some(ref peer) = self.state_sync_peer {
				let connected = self
					.peers
					.get_connected_peer(peer.info.addr)
					.map(|connected| Arc::ptr_eq(&connected, peer) && connected.is_connected())
					.unwrap_or(false);
				if !connected {
					let addr = peer.info.addr;
					self.schedule_retry();
					warn!("Peer connection lost: {}. Restarting sync.", addr);
				}
			} else {
				self.schedule_retry();
				warn!("Txhashset download has no associated peer. Restarting sync.");
			}
		}

		// run txhashset sync if applicable, normally only run one-time, except restart in error
		if self.state_sync_retry
			|| header_head.height == highest_height
			|| matches!(self.sync_state.status(), SyncStatus::TxHashsetDownload { .. })
		{
			let (go, download_timeout) = self.state_sync_due();

			if let SyncStatus::TxHashsetDownload { .. } = self.sync_state.status() {
				if download_timeout {
					if let Some(ref peer) = self.state_sync_peer {
						error!(
							"Txhashset download from {} made no progress for 20 minutes. Restarting sync.",
							peer.info.addr
						);
						peer.stop();
						let _ = self.peers.disconnect_peer(peer.info.addr);
					}
					self.sync_state.set_sync_error(
						chain::Error::SyncError(format!("{:?}", p2p::Error::Timeout)).into(),
					);
				}
			}

			if go {
				match self.request_state(header_head) {
					Ok(_) => {
						self.state_sync_retry = false;
						let now = Utc::now();
						self.sync_state.update(SyncStatus::TxHashsetDownload {
							start_time: now,
							prev_update_time: now,
							update_time: now,
							prev_downloaded_size: 0,
							downloaded_size: 0,
							total_size: 0,
						});
					}
					Err(e) => debug!("TxHashset request deferred: {:?}", e),
				}
			}
		}
		true
	}

	fn request_state(&mut self, header_head: &chain::Tip) -> Result<(), p2p::Error> {
		let (peer, txhashset_head) = self.state_peer(header_head)?;
		let bhash = txhashset_head.hash();
		debug!(
			"Requesting txhashset: head {} / {}, archive {} / {}, validated peer {}",
			header_head.height,
			header_head.last_block_h,
			txhashset_head.height,
			bhash,
			peer.info.addr,
		);
		peer.send_txhashset_request(
			txhashset_head.height,
			bhash,
			txhashset_archive_size_limit(&txhashset_head),
		)?;
		self.state_sync_peer = Some(peer);
		Ok(())
	}

	fn state_peer(
		&self,
		header_head: &chain::Tip,
	) -> Result<(Arc<Peer>, crate::core::core::BlockHeader), p2p::Error> {
		let threshold = global::state_sync_threshold() as u64;
		let archive_interval = global::txhashset_archive_interval();
		let mut txhashset_height = header_head.height.saturating_sub(threshold);
		txhashset_height = txhashset_height.saturating_sub(txhashset_height % archive_interval);

		let txhashset_head = self
			.chain
			.get_header_by_height(txhashset_height)
			.map_err(|e| {
				error!(
					"Chain error getting txhashset header at {}: {:?}",
					txhashset_height, e
				);
				p2p::Error::Internal
			})?;

		let peers = self
			.peers
			.outgoing_connected_peers()
			.into_iter()
			.filter(|peer| {
				let mut offered_height = peer.info.advertised_height().saturating_sub(threshold);
				offered_height -= offered_height % archive_interval;
				peer.info
					.capabilities
					.contains(p2p::Capabilities::TXHASHSET_HIST)
					&& offered_height == txhashset_head.height
			})
			.collect::<Vec<_>>();

		let peer = peers
			.iter()
			.filter(|peer| {
				let height = peer.info.validated_height();
				height >= txhashset_head.height
					&& self
						.chain
						.get_header_by_height(height)
						.map(|header| {
							peer.info.validated_tip_matches(
								header.height,
								&header.total_difficulty(),
								&header.hash(),
							)
						})
						.unwrap_or(false)
			})
			.max_by_key(|peer| {
				(
					self.state_sync_peer
						.as_ref()
						.map(|previous| previous.info.addr != peer.info.addr)
						.unwrap_or(true),
					peer.info.validated_total_difficulty(),
				)
			})
			.cloned();
		if let Some(peer) = peer {
			return Ok((peer, txhashset_head));
		}

		// A claim may authorize one bounded probe, never the state request itself.
		if let Some(peer) = peers
			.into_iter()
			.find(|peer| peer.info.header_probe_eligible())
		{
			let height = txhashset_head.height.saturating_sub(1);
			if let Ok(header) = self.chain.get_header_by_height(height) {
				if peer.send_bounded_header_request(vec![header.hash()]).is_err() {
					let _ = self.peers.disconnect_peer(peer.info.addr);
				}
			}
		}
		Err(p2p::Error::PeerException)
	}

	// For now this is a one-time thing (it can be slow) at initial startup.
	fn state_sync_due(&mut self) -> (bool, bool) {
		let now = Utc::now();
		let mut download_timeout = false;

		match self.sync_state.status() {
			SyncStatus::TxHashsetDownload {
				update_time,
				..
			} => {
				if now - update_time > Duration::minutes(20) {
					download_timeout = true;
				}
				(false, download_timeout)
			}
			_ => {
				let go = self
					.prev_state_sync
					.map(|prev| now - prev > Duration::seconds(10))
					.unwrap_or(true);
				if go {
					self.prev_state_sync = Some(now);
				}
				(go, download_timeout)
			}
		}
	}

	fn schedule_retry(&mut self) {
		if !self.state_sync_retry {
			self.prev_state_sync = None;
			self.state_sync_retry = true;
		}
		self.sync_state.update(SyncStatus::TxHashsetSetup);
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::chain::types::NoopAdapter;
	use crate::core::core::BlockHeader;
	use crate::core::{genesis, global, pow};
	use crate::core::pow::Difficulty;
	use crate::epic::sync::test::TestNode;
	use crate::p2p::Capabilities;
	use crate::util::StopState;

	fn mark_archive_provider(peer: &Arc<Peer>, header: &BlockHeader) {
		peer.info.update_validated_tip(
			header.height,
			header.total_difficulty(),
			header.hash(),
		);
		peer.info
			.update_advertised_tip(1, Difficulty::from_num(1), 0);
	}

	fn downloading(sync_state: &SyncState) {
		let now = Utc::now();
		sync_state.update(SyncStatus::TxHashsetDownload {
			start_time: now,
			prev_update_time: now,
			update_time: now,
			prev_downloaded_size: 1,
			downloaded_size: 1,
			total_size: 2,
		});
	}

	#[test]
	fn archive_limit_is_mmr_derived_and_hard_capped() {
		let mut header = crate::core::core::BlockHeader::default();
		assert_eq!(
			txhashset_archive_size_limit(&header),
			TXHASHSET_ARCHIVE_BASE_BYTES
		);
		header.output_mmr_size = 10;
		header.kernel_mmr_size = 20;
		assert_eq!(
			txhashset_archive_size_limit(&header),
			TXHASHSET_ARCHIVE_BASE_BYTES
				+ 10 * TXHASHSET_ARCHIVE_BYTES_PER_OUTPUT_POS
				+ 20 * TXHASHSET_ARCHIVE_BYTES_PER_KERNEL_POS
		);
		header.output_mmr_size = u64::MAX;
		assert_eq!(
			txhashset_archive_size_limit(&header),
			TXHASHSET_ARCHIVE_HARD_MAX_BYTES
		);
	}

	#[test]
	fn retry_leaves_download_state_and_keeps_request_cooldown() {
		let node = TestNode::with_outbound_peer(Capabilities::TXHASHSET_HIST);
		let now = Utc::now();
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::TxHashsetDownload {
			start_time: now,
			prev_update_time: now,
			update_time: now,
			prev_downloaded_size: 0,
			downloaded_size: 0,
			total_size: 1,
		});
		let mut state_sync = StateSync::new(
			sync_state.clone(),
			node.server.peers.clone(),
			node.chain.clone(),
		);

		state_sync.schedule_retry();

		assert!(matches!(sync_state.status(), SyncStatus::TxHashsetSetup));
		assert!(state_sync.state_sync_retry);
		assert!(state_sync.prev_state_sync.is_none());
		assert!(state_sync.state_sync_due().0);
		assert!(!state_sync.state_sync_due().0);
	}

	#[test]
	fn failed_request_does_not_enter_txhashset_download() {
		global::set_mining_mode(global::ChainTypes::AutomatedTesting);
		let root = std::env::temp_dir().join(format!(
			"epic-state-sync-test-{}-{}",
			std::process::id(),
			Utc::now().timestamp_nanos_opt().unwrap(),
		));
		let genesis = genesis::genesis_dev();
		let server = Arc::new(
			p2p::Server::new(
				root.join("peers").to_str().unwrap(),
				Capabilities::UNKNOWN,
				p2p::P2PConfig::default(),
				Arc::new(p2p::DummyAdapter {}),
				genesis.hash(),
				Arc::new(StopState::new()),
				None,
			)
			.unwrap(),
		);
		let chain = Arc::new(
			chain::Chain::init(
				root.join("chain").to_string_lossy().into_owned(),
				Arc::new(NoopAdapter {}),
				genesis,
				pow::verify_size,
				false,
			)
			.unwrap(),
		);
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::BodySync {
			current_height: 0,
			highest_height: 0,
		});
		let tip = chain.header_head().unwrap();
		let mut state_sync = StateSync::new(sync_state.clone(), server.peers.clone(), chain);

		state_sync.check_run(&tip, &tip, &tip, tip.height);
		assert!(matches!(sync_state.status(), SyncStatus::BodySync { .. }));
		drop(state_sync);
		drop(server);
		std::fs::remove_dir_all(root).unwrap();
	}

	#[test]
	fn successful_request_enters_txhashset_download() {
		let node = TestNode::with_outbound_peer(Capabilities::TXHASHSET_HIST);
		let sync_state = Arc::new(SyncState::new());
		let header = node.chain.get_header_by_height(0).unwrap();
		node.peer
			.info
			.update_advertised_tip(1, Difficulty::from_num(1), 0);
		let tip = chain::Tip::from_header(&header);
		let mut header_head = tip.clone();
		header_head.height = 1;
		sync_state.update(SyncStatus::BodySync {
			current_height: 0,
			highest_height: 1,
		});
		let mut state_sync = StateSync::new(
			sync_state.clone(),
			node.server.peers.clone(),
			node.chain.clone(),
		);
		assert!(state_sync.state_peer(&header_head).is_err());
		assert!(!node.peer.info.header_probe_eligible());
		node.peer.info.update_validated_tip(
			header.height,
			header.total_difficulty(),
			header.hash(),
		);

		state_sync.check_run(&header_head, &tip, &tip, 1);
		assert!(matches!(
			sync_state.status(),
			SyncStatus::TxHashsetDownload { .. }
		));
	}

	#[test]
	fn validated_canonical_ancestor_is_an_eligible_provider() {
		let mut node = TestNode::with_outbound_peer(Capabilities::TXHASHSET_HIST);
		let malicious = node.peer.clone();
		let honest = node.add_outbound_peer(Capabilities::TXHASHSET_HIST);
		let header = node.chain.get_header_by_height(0).unwrap();
		honest.info.update_validated_tip(
			header.height,
			header.total_difficulty(),
			header.hash(),
		);
		honest
			.info
			.update_advertised_tip(1, Difficulty::from_num(1), 0);
		malicious
			.info
			.update_advertised_tip(1, Difficulty::from_num(1_000_000), 0);
		let mut later_tip = chain::Tip::from_header(&header);
		later_tip.height = 1;
		let state_sync = StateSync::new(
			Arc::new(SyncState::new()),
			node.server.peers.clone(),
			node.chain.clone(),
		);

		let (peer, archive) = state_sync.state_peer(&later_tip).unwrap();
		assert!(Arc::ptr_eq(&peer, &honest));
		assert_eq!(archive.height, 0);
	}

	#[test]
	fn validated_provider_requires_txhashset_capability() {
		let node = TestNode::with_outbound_peer(Capabilities::UNKNOWN);
		let header = node.chain.get_header_by_height(0).unwrap();
		node.peer.info.update_validated_tip(
			header.height,
			header.total_difficulty(),
			header.hash(),
		);
		node.peer
			.info
			.update_advertised_tip(1, Difficulty::from_num(1), 0);
		let mut later_tip = chain::Tip::from_header(&header);
		later_tip.height = 1;
		let state_sync = StateSync::new(
			Arc::new(SyncState::new()),
			node.server.peers.clone(),
			node.chain.clone(),
		);

		assert!(state_sync.state_peer(&later_tip).is_err());
	}

	#[test]
	fn retry_prefers_a_different_validated_provider() {
		let mut node = TestNode::with_outbound_peer(Capabilities::TXHASHSET_HIST);
		let first = node.peer.clone();
		let second = node.add_outbound_peer(Capabilities::TXHASHSET_HIST);
		let header = node.chain.get_header_by_height(0).unwrap();
		for peer in [&first, &second] {
			peer.info.update_validated_tip(
				header.height,
				header.total_difficulty(),
				header.hash(),
			);
			peer.info
				.update_advertised_tip(1, Difficulty::from_num(1), 0);
		}
		let mut later_tip = chain::Tip::from_header(&header);
		later_tip.height = 1;
		let mut state_sync = StateSync::new(
			Arc::new(SyncState::new()),
			node.server.peers.clone(),
			node.chain.clone(),
		);
		state_sync.state_sync_peer = Some(first);

		let (peer, _) = state_sync.state_peer(&later_tip).unwrap();
		assert!(Arc::ptr_eq(&peer, &second));
	}

	#[test]
	fn validation_failure_retries_with_a_different_provider() {
		let mut node = TestNode::with_outbound_peer(Capabilities::TXHASHSET_HIST);
		let first = node.peer.clone();
		let second = node.add_outbound_peer(Capabilities::TXHASHSET_HIST);
		let header = node.chain.get_header_by_height(0).unwrap();
		for peer in [&first, &second] {
			peer.info.update_validated_tip(
				header.height,
				header.total_difficulty(),
				header.hash(),
			);
			peer.info
				.update_advertised_tip(1, Difficulty::from_num(1), 0);
		}
		let tip = chain::Tip::from_header(&header);
		let mut header_head = tip.clone();
		header_head.height = 1;
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::TxHashsetSetup);
		sync_state.set_sync_error(
			chain::Error::InvalidTxHashSet("invalid archive".to_owned()).into(),
		);
		let mut state_sync = StateSync::new(
			sync_state.clone(),
			node.server.peers.clone(),
			node.chain.clone(),
		);
		state_sync.state_sync_peer = Some(first);

		state_sync.check_run(&header_head, &tip, &tip, 1);

		assert!(Arc::ptr_eq(
			state_sync.state_sync_peer.as_ref().unwrap(),
			&second
		));
		assert!(matches!(
			sync_state.status(),
			SyncStatus::TxHashsetDownload { .. }
		));
	}

	#[test]
	fn dropped_archive_connection_retries_with_a_different_provider() {
		let mut node = TestNode::with_outbound_peer(Capabilities::TXHASHSET_HIST);
		let first = node.peer.clone();
		let second = node.add_outbound_peer(Capabilities::TXHASHSET_HIST);
		let header = node.chain.get_header_by_height(0).unwrap();
		for peer in [&first, &second] {
			mark_archive_provider(peer, &header);
		}
		let tip = chain::Tip::from_header(&header);
		let mut header_head = tip.clone();
		header_head.height = 1;
		let sync_state = Arc::new(SyncState::new());
		downloading(&sync_state);
		let mut state_sync = StateSync::new(
			sync_state.clone(),
			node.server.peers.clone(),
			node.chain.clone(),
		);
		state_sync.state_sync_peer = Some(first.clone());
		state_sync.prev_state_sync = Some(Utc::now());

		node.server.peers.disconnect_peer(first.info.addr).unwrap();
		assert!(first.is_connected(), "retained peer exposes the stale state");

		state_sync.check_run(&header_head, &tip, &tip, 1);

		assert!(Arc::ptr_eq(
			state_sync.state_sync_peer.as_ref().unwrap(),
			&second
		));
		assert!(matches!(
			sync_state.status(),
			SyncStatus::TxHashsetDownload { .. }
		));
	}

	#[test]
	fn replacement_connection_cannot_keep_an_old_archive_request_alive() {
		let mut node = TestNode::with_outbound_peer(Capabilities::TXHASHSET_HIST);
		let first = node.peer.clone();
		let second = node.add_outbound_peer(Capabilities::TXHASHSET_HIST);
		let header = node.chain.get_header_by_height(0).unwrap();
		for peer in [&first, &second] {
			mark_archive_provider(peer, &header);
		}
		let tip = chain::Tip::from_header(&header);
		let mut header_head = tip.clone();
		header_head.height = 1;
		let sync_state = Arc::new(SyncState::new());
		downloading(&sync_state);
		let mut state_sync = StateSync::new(
			sync_state,
			node.server.peers.clone(),
			node.chain.clone(),
		);
		state_sync.state_sync_peer = Some(first.clone());
		state_sync.prev_state_sync = Some(Utc::now());

		let replacement = node.reconnect_outbound_peer();
		assert_eq!(replacement.info.addr, first.info.addr);
		assert!(!Arc::ptr_eq(&replacement, &first));

		state_sync.check_run(&header_head, &tip, &tip, 1);

		assert!(Arc::ptr_eq(
			state_sync.state_sync_peer.as_ref().unwrap(),
			&second
		));
	}

	#[test]
	fn connected_archive_peer_does_not_bypass_download_watchdog() {
		let mut node = TestNode::with_outbound_peer(Capabilities::TXHASHSET_HIST);
		let first = node.peer.clone();
		let second = node.add_outbound_peer(Capabilities::TXHASHSET_HIST);
		let header = node.chain.get_header_by_height(0).unwrap();
		for peer in [&first, &second] {
			mark_archive_provider(peer, &header);
		}
		let tip = chain::Tip::from_header(&header);
		let mut header_head = tip.clone();
		header_head.height = 1;
		let sync_state = Arc::new(SyncState::new());
		let stalled_at = Utc::now() - Duration::minutes(21);
		sync_state.update(SyncStatus::TxHashsetDownload {
			start_time: stalled_at,
			prev_update_time: stalled_at,
			update_time: stalled_at,
			prev_downloaded_size: 0,
			downloaded_size: 1,
			total_size: 2,
		});
		let mut state_sync = StateSync::new(
			sync_state,
			node.server.peers.clone(),
			node.chain.clone(),
		);
		state_sync.state_sync_peer = Some(first.clone());
		state_sync.prev_state_sync = Some(stalled_at);

		state_sync.check_run(&header_head, &tip, &tip, 2);
		assert!(node
			.server
			.peers
			.get_connected_peer(first.info.addr)
			.is_none());
		assert!(!first.is_banned());
		state_sync.check_run(&header_head, &tip, &tip, 2);

		assert!(Arc::ptr_eq(
			state_sync.state_sync_peer.as_ref().unwrap(),
			&second
		));
	}

	#[test]
	fn active_archive_connection_does_not_request_another_provider() {
		let mut node = TestNode::with_outbound_peer(Capabilities::TXHASHSET_HIST);
		let first = node.peer.clone();
		let second = node.add_outbound_peer(Capabilities::TXHASHSET_HIST);
		let header = node.chain.get_header_by_height(0).unwrap();
		for peer in [&first, &second] {
			mark_archive_provider(peer, &header);
		}
		let tip = chain::Tip::from_header(&header);
		let mut header_head = tip.clone();
		header_head.height = 1;
		let sync_state = Arc::new(SyncState::new());
		downloading(&sync_state);
		let mut state_sync = StateSync::new(
			sync_state,
			node.server.peers.clone(),
			node.chain.clone(),
		);
		state_sync.state_sync_peer = Some(first.clone());

		state_sync.check_run(&header_head, &tip, &tip, 1);

		assert!(Arc::ptr_eq(
			state_sync.state_sync_peer.as_ref().unwrap(),
			&first
		));
	}

	#[test]
	fn provider_advertising_a_different_archive_interval_is_rejected() {
		let node = TestNode::with_outbound_peer(Capabilities::TXHASHSET_HIST);
		node.persist_header_head(40, 100_000);
		let header = node.chain.get_header_by_height(40).unwrap();
		node.peer.info.update_validated_tip(
			header.height,
			header.total_difficulty(),
			header.hash(),
		);
		node.peer.info.mark_header_probe_started();
		let header_head = chain::Tip::from_header(&header);
		let state_sync = StateSync::new(
			Arc::new(SyncState::new()),
			node.server.peers.clone(),
			node.chain.clone(),
		);

		node.peer
			.info
			.update_advertised_tip(40, Difficulty::from_num(100_000), 0);
		let (peer, archive) = state_sync.state_peer(&header_head).unwrap();
		assert!(Arc::ptr_eq(&peer, &node.peer));
		assert_eq!(archive.height, 20);

		node.peer
			.info
			.update_advertised_tip(30, Difficulty::from_num(100_000), 0);
		assert!(state_sync.state_peer(&header_head).is_err());

		node.peer
			.info
			.update_advertised_tip(50, Difficulty::from_num(100_000), 0);
		assert!(state_sync.state_peer(&header_head).is_err());
	}
}
