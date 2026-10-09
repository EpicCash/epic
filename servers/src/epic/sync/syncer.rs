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

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::thread;
use std::time;

use crate::chain::{self, SyncState, SyncStatus};
use crate::core::core::hash::Hashed;
use crate::core::core::BlockHeader;
use crate::core::pow::Difficulty;
use crate::epic::sync::body_sync::BodySync;
use crate::epic::sync::header_sync::{get_locator_heights, HeaderSync};
use crate::epic::sync::state_sync::StateSync;
use crate::p2p::{self, PeerInfo};
use crate::util::StopState;

struct FastsyncHeaderBatch {
	peer: Arc<p2p::Peer>,
	headers: Vec<BlockHeader>,
}

struct PendingHeaderSync {
	sync: HeaderSync,
	expected_height: u64,
	peer: Arc<p2p::Peer>,
}

//TODO: (Biz) change the name of this function maybe? It's imprecise now that we 
// use `SyncStatus::Initial` more than one place. It's now a restart sanity state
fn resume_sync_after_peer_wait(sync_state: &SyncState) {
	if matches!(sync_state.status(), SyncStatus::AwaitingPeers(_)) {
		sync_state.update(SyncStatus::Initial);
	}
}

fn queue_completed_header_batch(
	peers: &p2p::Peers,
	pending: PendingHeaderSync,
	headers: Vec<BlockHeader>,
	queue: &mut HashMap<u64, FastsyncHeaderBatch>,
) {
	if headers.is_empty() {
		return;
	}
	if headers[0].height != pending.expected_height {
		debug!(
			"sync: unexpected header batch from {} at {}, expected {}",
			pending.peer.info.addr, headers[0].height, pending.expected_height
		);
		let _ = peers.disconnect_peer(pending.peer.info.addr);
		return;
	}
	queue.insert(
		pending.expected_height,
		FastsyncHeaderBatch {
			peer: pending.peer,
			headers,
		},
	);
}

fn take_next_header_batch(
	queue: &mut HashMap<u64, FastsyncHeaderBatch>,
	next_height: u64,
) -> Option<FastsyncHeaderBatch> {
	queue.remove(&next_height)
}

fn header_batch_start(header_height: u64, offset: u8) -> u64 {
	header_height
		.saturating_add(offset as u64 * p2p::MAX_BLOCK_HEADERS as u64)
		.saturating_add(1)
}

fn header_batch_preserves_window(
	start_height: u64,
	highest_height: u64,
	headers_len: usize,
) -> bool {
	let max_headers = p2p::MAX_BLOCK_HEADERS as usize;
	headers_len == max_headers
		|| (headers_len > 0
			&& headers_len < max_headers
			&& start_height
				.saturating_add(p2p::MAX_BLOCK_HEADERS as u64 - 1)
				> highest_height)
}

fn next_header_offset(
	header_height: u64,
	highest_height: u64,
	occupied: &HashSet<u64>,
) -> Option<u8> {
	(0..=u8::MAX).find(|offset| {
		let height = header_batch_start(header_height, *offset);
		height <= highest_height && !occupied.contains(&height)
	})
}

fn validated_outbound_sync_peer(
	peer_infos: impl IntoIterator<Item = PeerInfo>,
	minimum_difficulty: &Difficulty,
) -> Option<PeerInfo> {
	peer_infos
		.into_iter()
		.filter(|peer| peer.is_outbound())
		.filter(|peer| peer.validated_total_difficulty() > minimum_difficulty.clone())
		.max_by_key(|peer| peer.validated_total_difficulty())
}

fn advertised_outbound_probe_peer(
	peer_infos: impl IntoIterator<Item = PeerInfo>,
	local_difficulty: &Difficulty,
) -> Option<PeerInfo> {
	peer_infos
		.into_iter()
		.filter(|peer| peer.is_outbound())
		.filter(|peer| peer.header_probe_eligible())
		.filter(|peer| peer.advertised_total_difficulty() > local_difficulty.clone())
		.max_by_key(|peer| peer.advertised_total_difficulty())
}

fn bounded_probe_height(header_height: u64, peer_info: &PeerInfo) -> u64 {
	let base_height = std::cmp::max(header_height, peer_info.validated_height());
	if !peer_info.header_probe_eligible() {
		return base_height;
	}
	let advertised_height = peer_info.advertised_height();
	let max_height = base_height.saturating_add(p2p::MAX_BLOCK_HEADERS as u64);
	if advertised_height > base_height {
		std::cmp::min(advertised_height, max_height)
	} else if peer_info.advertised_total_difficulty()
		> peer_info.validated_total_difficulty()
	{
		max_height
	} else {
		base_height
	}
}

fn header_sync_peer_eligible(
	peer_info: &PeerInfo,
	header_height: u64,
	target_peer: Option<std::net::SocketAddr>,
	has_queued_batch: bool,
	has_pending_request: bool,
) -> bool {
	!has_pending_request
		&& (has_queued_batch || peer_info.header_probe_eligible())
		&& (peer_info.advertised_height() > header_height
			|| target_peer == Some(peer_info.addr.0))
}

// Bound speculative HeaderSync work to one request per eligible outbound peer.
fn header_sync_window_height(
	header_height: u64,
	peer_infos: impl IntoIterator<Item = PeerInfo>,
) -> u64 {
	let mut peers = peer_infos
		.into_iter()
		.filter(|peer| {
			peer.is_outbound()
				&& peer.header_probe_eligible()
				&& peer.advertised_height() > header_height
		})
		.collect::<Vec<_>>();
	peers.sort_by_key(|peer| Reverse(peer.advertised_height()));
	if peers.iter().any(|peer| {
		peer.capabilities
			.contains(p2p::types::Capabilities::HEADER_FASTSYNC)
	}) {
		peers.retain(|peer| {
			peer.capabilities
				.contains(p2p::types::Capabilities::HEADER_FASTSYNC)
		});
	} else {
		peers.truncate(1);
	}

	let max_height = peers
		.first()
		.map(|peer| peer.advertised_height())
		.unwrap_or(header_height);
	let mut target = header_height;
	for peer in &peers {
		let next = target.saturating_add(p2p::MAX_BLOCK_HEADERS as u64);
		if peer.advertised_height() < next {
			break;
		}
		target = next;
	}

	if target == header_height {
		std::cmp::min(
			max_height,
			header_height.saturating_add(p2p::MAX_BLOCK_HEADERS as u64),
		)
	} else {
		target
	}
}

pub fn run_sync(
	sync_state: Arc<SyncState>,
	peers: Arc<p2p::Peers>,
	chain: Arc<chain::Chain>,
	stop_state: Arc<StopState>,
) -> std::io::Result<std::thread::JoinHandle<()>> {
	thread::Builder::new()
		.name("sync".to_string())
		.spawn(move || {
			let runner = SyncRunner::new(sync_state, peers, chain, stop_state);
			runner.sync_loop();
		})
}

struct SyncRunner {
	sync_state: Arc<SyncState>,
	peers: Arc<p2p::Peers>,
	chain: Arc<chain::Chain>,
	stop_state: Arc<StopState>,
}

impl SyncRunner {
	fn new(
		sync_state: Arc<SyncState>,
		peers: Arc<p2p::Peers>,
		chain: Arc<chain::Chain>,
		stop_state: Arc<StopState>,
	) -> SyncRunner {
		SyncRunner {
			sync_state,
			peers,
			chain,
			stop_state,
		}
	}

	fn wait_for_min_peers(&self) -> Result<(), chain::Error> {
		// Initial sleep to give us time to peer with some nodes.
		let wait_secs = 30;
		let peers_config = self.peers.get_config();
		let mut n = 0;
		loop {
			if self.stop_state.is_stopped() {
				break;
			}

			// Check if there are enough outbound peers
			if self.peers.enough_outbound_peers() {
				break;
			}

			if n > wait_secs {
				n = 0;
				warn!(
					"Waiting for the minimum number of preferred outbound peers. required/current: {:?}/{:?} - see epic config: peer_min_preferred_outbound_count",
					peers_config.peer_min_preferred_outbound_count(),
					self.peers.peer_outbound_count()
				);
			}

			thread::sleep(time::Duration::from_secs(1));
			n += 1;
		}

		Ok(())
	}

	/// Starts the syncing loop, just spawns two threads that loop forever
	fn sync_loop(&self) {
		macro_rules! unwrap_or_restart_loop(
    	  ($obj: expr) =>(
    		match $obj {
    			Ok(v) => v,
    			Err(e) => {
    				error!("unexpected error: {:?}", e);
    				thread::sleep(time::Duration::from_secs(1));
    				continue;
    			},
    		}
    	));

		// Wait for connections reach at least MIN_PEERS
		// Ensure we have the minimum number of peers before proceeding

		match self.wait_for_min_peers() {
			Ok(_) => {
				info!("Minimum peers requirement met, proceeding with sync.");
				resume_sync_after_peer_wait(self.sync_state.as_ref());
			}
			Err(e) => {
				error!("wait_for_min_peers failed: {:?}", e);
				// If the minimum peers requirement is not met, log and restart the loop
				return; // Beende die aktuelle Iteration der Schleife
			}
		}

		// Our 3 main sync stages
		let mut header_syncs: HashMap<p2p::PeerAddr, PendingHeaderSync> = HashMap::new();
		let mut fastsync_header_queue: HashMap<u64, FastsyncHeaderBatch> = HashMap::new();

		let mut download_headers = false;

		let mut body_sync = BodySync::new(
			self.sync_state.clone(),
			self.peers.clone(),
			self.chain.clone(),
		);
		let mut state_sync = StateSync::new(
			self.sync_state.clone(),
			self.peers.clone(),
			self.chain.clone(),
		);

		// Highest height seen on the network, generally useful for a fast test on
		// whether some sync is needed
		let mut highest_network_height = 0;
		// Main syncing loop
		loop {
			// Check if the node is shutting down then exit the loop
			if self.stop_state.is_stopped() {
				break;
			}

			// Check if there are enough outbound peers, else restart loop from here
			if !self.peers.enough_outbound_peers() {
				warn!("Not enough outbound peers available. Waiting for more peers to connect...");
				self.sync_state.update(SyncStatus::AwaitingPeers(true));
				thread::sleep(time::Duration::from_secs(5));
				continue; // Skip the current iteration of the loop
			}
			resume_sync_after_peer_wait(self.sync_state.as_ref());

			let has_synstatus = self.sync_state.is_syncing();

			// Check whether syncing is generally needed by comparing our state with others
			let (needs_syncing, most_work_height, target_peer) = unwrap_or_restart_loop!(
				self.needs_syncing_with_pending(!header_syncs.is_empty())
			);

			if most_work_height > 0 {
				// Occasionally, we can get a most work height of 0 if read locks fail
				let active_window = !header_syncs.is_empty() || !fastsync_header_queue.is_empty();
				highest_network_height = if active_window {
					std::cmp::max(highest_network_height, most_work_height)
				} else {
					most_work_height
				};
			}

			let sleep_duration = match self.sync_state.status() {
				SyncStatus::HeaderSync { .. }
				| SyncStatus::Initial
				| SyncStatus::AwaitingPeers(_) => {
					time::Duration::from_millis(100)
				}
				SyncStatus::BodySync { .. } => time::Duration::from_millis(100),
				_ => time::Duration::from_secs(10),
			};

			// Quick short-circuit (and a decent sleep) if no syncing is needed
			let mut needs_headersync = false;
			if !needs_syncing {
				if has_synstatus {
					// Transition out of a "syncing" state and into NoSync
					self.sync_state.update(SyncStatus::Compacting);
					// Initial transition out of a "syncing" state and into NoSync.
					// This triggers a chain compaction to keep our local node tidy.
					// Note: Chain compaction runs with an internal threshold
					// so it can be safely run even if the node is restarted frequently.
					unwrap_or_restart_loop!(self.chain.compact());
					self.sync_state.update(SyncStatus::NoSync);
				}
				self.request_header_probe();

				// Sleep for 10 seconds but check the stop signal every second
				for _ in 1..10 {
					thread::sleep(time::Duration::from_secs(1));
					if self.stop_state.is_stopped() {
						break;
					}
				}
				continue;
			} else if matches!(
				self.sync_state.status(),
				SyncStatus::NoSync | SyncStatus::Initial | SyncStatus::AwaitingPeers(_)
			)
				&& most_work_height > self.chain.header_head().map(|tip| tip.height).unwrap_or(0)
			{
				warn!("Node is out of sync, switching to syncing mode");
				needs_headersync = true;
			}

			thread::sleep(sleep_duration);

			// If syncing is needed
			let head = unwrap_or_restart_loop!(self.chain.head());
			let tail = self.chain.tail().unwrap_or_else(|_| head.clone());
			let header_head = unwrap_or_restart_loop!(self.chain.header_head());

			let mut txhashset_sync = false;
			// Run each sync stage, each of them deciding whether they're needed
			// except for state sync that only runs if body sync returns true (meaning txhashset is needed)

			if needs_headersync {
				self.sync_state.update(SyncStatus::HeaderSync {
					current_height: head.height,
					highest_height: highest_network_height,
				});

				let _ = self.chain.reset_sync_head();
				// Rebuild the sync MMR to match our updated sync_head.
				let _ = self.chain.rebuild_sync_mmr(&header_head);
				download_headers = true;
			}

			if matches!(self.sync_state.status(), SyncStatus::HeaderSync { .. }) {
				let mut completed = vec![];
				for (peer_addr, pending) in &mut header_syncs {
					match pending.sync.check_run() {
						Ok((headers, peer_blocks)) if peer_blocks || !headers.is_empty() => {
							completed.push((*peer_addr, headers));
						}
						Err(_) => completed.push((*peer_addr, vec![])),
						_ => {}
					}
				}

				for (peer_addr, headers) in completed {
					if let Some(pending) = header_syncs.remove(&peer_addr) {
						queue_completed_header_batch(
							&self.peers,
							pending,
							headers,
							&mut fastsync_header_queue,
						);
					}
				}
			}

			if download_headers && matches!(self.sync_state.status(), SyncStatus::HeaderSync { .. }) {
				let mut occupied = header_syncs
					.values()
					.map(|pending| pending.expected_height)
					.chain(fastsync_header_queue.keys().copied())
					.collect::<HashSet<_>>();
				let queued_peers = fastsync_header_queue
					.values()
					.map(|batch| batch.peer.clone())
					.collect::<Vec<_>>();
				let mut peers = self
					.peers
					.outgoing_connected_peers()
					.into_iter()
					.filter(|peer| {
						peer.is_connected()
							&& !peer.is_banned()
							&& header_sync_peer_eligible(
								&peer.info,
								header_head.height,
								target_peer,
								queued_peers
									.iter()
									.any(|queued| Arc::ptr_eq(queued, peer)),
								header_syncs.contains_key(&peer.info.addr),
							)
					})
					.collect::<Vec<_>>();
				peers.sort_by_key(|peer| {
					(
						queued_peers
							.iter()
							.any(|queued| Arc::ptr_eq(queued, peer)),
						Reverse(peer.info.advertised_height()),
					)
				});
				let fastsync = peers.iter().any(|peer| {
					peer.info
						.capabilities
						.contains(p2p::types::Capabilities::HEADER_FASTSYNC)
				});
				if fastsync {
					peers.retain(|peer| {
						peer.info
							.capabilities
							.contains(p2p::types::Capabilities::HEADER_FASTSYNC)
					});
				} else {
					peers.truncate(1);
				}

				for peer in peers {
					let peer_addr = peer.info.addr;
					let request_offset = next_header_offset(
						header_head.height,
						highest_network_height,
						&occupied,
					);
					if let Some(offset) = request_offset {
						let expected_height = header_batch_start(header_head.height, offset);
						let mut header_sync = HeaderSync::new(
							self.sync_state.clone(),
							self.peers.clone(),
							peer.clone(),
							self.chain.clone(),
							header_head.height,
							highest_network_height,
							offset,
						);
						match header_sync.check_run() {
							Ok((headers, false)) if headers.is_empty() => {
								occupied.insert(expected_height);
								header_syncs.insert(
									peer_addr,
									PendingHeaderSync {
										sync: header_sync,
										expected_height,
										peer: peer.clone(),
									},
								);
							}
							Ok((headers, _))
								if headers.first().map(|header| header.height)
									== Some(expected_height) =>
							{
								occupied.insert(expected_height);
								fastsync_header_queue.insert(
									expected_height,
									FastsyncHeaderBatch {
										peer: peer.clone(),
										headers,
									},
								);
							}
							Ok((headers, _)) if !headers.is_empty() => {
								let _ = self.peers.disconnect_peer(peer_addr);
							}
							_ => {}
						}
					}
				}
			}

			let mut next_height = header_head.height.saturating_add(1);
			while let Some(batch) =
				take_next_header_batch(&mut fastsync_header_queue, next_height)
			{
				let peer_addr = batch.peer.info.addr;
				if !header_batch_preserves_window(
					next_height,
					highest_network_height,
					batch.headers.len(),
				) {
					warn!(
						"sync: rejecting incomplete non-terminal header batch from {} at {}: received {}, expected {}",
						peer_addr,
						next_height,
						batch.headers.len(),
						p2p::MAX_BLOCK_HEADERS
					);
					batch.peer.stop();
					header_syncs.retain(|_, pending| !Arc::ptr_eq(&pending.peer, &batch.peer));
					fastsync_header_queue
						.retain(|_, queued| !Arc::ptr_eq(&queued.peer, &batch.peer));
					break;
				}
				let following_height = batch
					.headers
					.last()
					.map(|header| header.height.saturating_add(1))
					.unwrap_or(next_height);
				match self
					.peers
					.adapter
					.headers_received(&batch.headers, &batch.peer.info)
				{
					Ok(true) => {
						next_height = following_height;
						info!(
							"Header sync verified through height {}",
							next_height.saturating_sub(1)
						);
					}
					Ok(false) => {
						if let Err(e) = self
							.peers
							.ban_peer(peer_addr, p2p::types::ReasonForBan::BadBlockHeader)
						{
							warn!("Failed to ban peer {}: {:?}", peer_addr, e);
						}
						break;
					}
					Err(err) => {
						error!(
							"Chainsync: failed to process received headers from peer {} at height {}: {:?}.",
							peer_addr, next_height, err
						);
						let _ = self.peers.disconnect_peer(peer_addr);
						break;
					}
				}
			}

			match self.sync_state.status() {
				SyncStatus::Compacting => {
					// Während der Kompaktierung keine anderen Prozesse ausführen
					thread::sleep(time::Duration::from_secs(1));
					continue;
				}
				SyncStatus::Shutdown => {
					download_headers = false;
					continue;
				}

				SyncStatus::TxHashsetDownload { .. }
				| SyncStatus::TxHashsetSetup
				| SyncStatus::TxHashsetRangeProofsValidation { .. }
				| SyncStatus::TxHashsetKernelsValidation { .. }
				| SyncStatus::TxHashsetSave => txhashset_sync = true,

				SyncStatus::TxHashsetDone => {
					// if txhashset is downloaded replaced with own txhashet we go to body sync.
					// because download and validatet requires very long we missed new headers

					// Update highest_network_height before transitioning to HeaderSync
					let (_needs_syncing, most_work_height, _target_peer) =
						unwrap_or_restart_loop!(self.needs_syncing_with_pending(false));

					if most_work_height > 0 {
						highest_network_height = most_work_height;
						info!(
							"Updated highest_network_height to {} before transitioning to HeaderSync",
							highest_network_height
						);
					} else {
						warn!(
							"Failed to update highest_network_height, keeping previous value: {}",
							highest_network_height
						);
					}
					// if we are done with txhashset sync, we can start header sync
					// reset sync head to header_head
					// and start header sync
					let sync_head = 
						unwrap_or_restart_loop!(self.chain.get_sync_head());
					info!(
						"Check transition to HeaderSync. Head {} at {}, resetting to: {} at {}",
						sync_head.hash(),
						sync_head.height,
						header_head.hash(),
						header_head.height,
					);

					let _ = self.chain.reset_sync_head();

					// Rebuild the sync MMR to match our updated sync_head.
					let _ = self.chain.rebuild_sync_mmr(&header_head);
					self.sync_state.update(SyncStatus::BodySync {
						current_height: head.height,
						highest_height: highest_network_height,
					});
					download_headers = false;
					continue;
				}

				SyncStatus::AwaitingPeers(_) => {
					// Only start header download if enough outbound peers are available

					if self.peers.enough_outbound_peers() && !download_headers {
						let sync_head = unwrap_or_restart_loop!(self.chain.get_sync_head());
						info!(
							"Initial transition to HeaderSync. Head {} at {}, resetting to: {} at {}",
							sync_head.hash(),
							sync_head.height,
							header_head.hash(),
							header_head.height,
						);

						// If already at the same head, skip header sync and go to body sync
						if highest_network_height > 0
							&& header_head.height >= highest_network_height
							&& sync_head.hash() == header_head.hash()
							&& sync_head.height == header_head.height
						{
							info!("Header sync head unchanged, proceeding directly to BodySync.");
							self.sync_state.update(SyncStatus::BodySync {
								current_height: sync_head.height,
								highest_height: highest_network_height,
							});
							download_headers = false;
							continue;
						}

						let _ = self.chain.reset_sync_head();
						let _ = self.chain.rebuild_sync_mmr(&header_head);
						download_headers = true;
						//set to HEaderSync
						self.sync_state.update(SyncStatus::HeaderSync {
							current_height: head.height,
							highest_height: highest_network_height,
						});
					} else {
						download_headers = false;
					}
				}

				_ => {
					if header_head.height >= highest_network_height {
						// Header-Synchronisierung abgeschlossen

						// Wechsel zu Body-Synchronisierung
						self.sync_state.update(SyncStatus::BodySync {
							current_height: head.height,
							highest_height: highest_network_height,
						});
						download_headers = false;
						header_syncs.clear();
						fastsync_header_queue.clear();

						let check_run = match body_sync.check_run(&head, highest_network_height) {
							Ok(v) => v,
							Err(e) => {
								error!("check_run failed: {:?}", e);
								continue;
							}
						};

						if check_run {
							txhashset_sync = true;
						}
					} else {
						download_headers = true;

						continue;
					}
				}
			}

			//txhashset download
			//TODO: rename state_sync to txhashset_sync
			//if we are in txhashset sync state and we are not in body sync state, run state sync
			if txhashset_sync {
				state_sync.check_run(&header_head, &head, &tail, highest_network_height);
			}
		}
	}

	/// Whether we're currently syncing the chain or we're fully caught up and
	/// just receiving blocks through gossip.
	#[cfg(test)]
	fn needs_syncing(&self) -> Result<(bool, u64, Option<std::net::SocketAddr>), chain::Error> {
		self.needs_syncing_with_pending(false)
	}

	fn needs_syncing_with_pending(
		&self,
		header_request_pending: bool,
	) -> Result<(bool, u64, Option<std::net::SocketAddr>), chain::Error> {
		let local_head = self.chain.head()?;
		let local_diff = local_head.total_difficulty.clone();
		let header_head = self.chain.header_head()?;
		let header_height = header_head.height;
		let status = self.sync_state.status();

		// Start or resume with one bounded HeaderSync batch. Peer validation is
		// connection-scoped, while the locally validated header head survives a
		// restart, so compare fresh advertisements to that persisted head.
		// The advertisement selects the outbound peer but grants no authority
		// beyond the headers that the chain subsequently validates.
		if matches!(status, SyncStatus::Initial | SyncStatus::AwaitingPeers(_))
			|| (status == SyncStatus::NoSync && local_head.height == 0 && header_height == 0)
		{
			if let Some(peer_info) = advertised_outbound_probe_peer(
				self.peers
					.outgoing_connected_peers()
					.into_iter()
					.map(|peer| peer.info.clone()),
				&header_head.total_difficulty,
			) {
				debug!(
					"sync: starting bounded startup HeaderSync with {}",
					peer_info.addr
				);
				return Ok((
					true,
					bounded_probe_height(header_height, &peer_info),
					Some(peer_info.addr.0),
				));
			}
			if header_height > local_head.height {
				return Ok((true, header_height, None));
			}
		}

		let validated_peer_infos = self
			.peers
			.connected_peers()
			.into_iter()
			.map(|peer| peer.info.clone())
			.filter(|peer| {
				self.chain
					.get_header_by_height(peer.validated_height())
					.map(|header| {
						peer.validated_tip_matches(
							header.height,
							&header.total_difficulty(),
							&header.hash(),
						)
					})
					.unwrap_or(false)
			})
			.collect::<Vec<_>>();

		// A validated header head is durable across restarts and peer reconnects.
		// Never declare the node synchronized while its body is still behind it.
		// HeaderSync is excluded so validated peers can continue advancing headers.
		if !matches!(status, SyncStatus::HeaderSync { .. })
			&& header_head.total_difficulty.clone() > local_diff.clone()
		{
			debug!(
				"sync: local header head at {} is ahead of body head at {}, keeping sync active",
				header_height, local_head.height
			);
			let target_peer = validated_outbound_sync_peer(
				validated_peer_infos.iter().cloned(),
				&local_diff,
			)
			.map(|peer| peer.addr.0);
			return Ok((true, header_height, target_peer));
		}

		if status == SyncStatus::NoSync {
			// Retain the existing five-block threshold for entering HeaderSync.
			let threshold = {
				let diff_iter = match self.chain.difficulty_iter() {
					Ok(v) => v,
					Err(e) => {
						error!("failed to get difficulty iterator: {:?}", e);
						return Ok((false, 0, None));
					}
				};
				diff_iter
					.map(|x| x.difficulty)
					.take(5)
					.fold(Difficulty::zero(), |sum, val| sum + val)
			};
			let minimum_difficulty = local_diff.clone() + threshold.clone();
			if let Some(peer_info) = validated_outbound_sync_peer(
				validated_peer_infos.iter().cloned(),
				&minimum_difficulty,
			)
			{
				debug!(
					"sync: total_difficulty {}, peer_validated_difficulty {}, threshold {} (last 5 blocks), enabling sync",
					local_diff,
					peer_info.validated_total_difficulty(),
					threshold,
				);
				let target = bounded_probe_height(header_height, &peer_info);
				return Ok((true, target, Some(peer_info.addr.0)));
			}
			if header_height > local_head.height {
				if let Some(peer_info) = validated_outbound_sync_peer(
					validated_peer_infos.iter().cloned(),
					&local_diff,
				)
				{
					return Ok((true, header_height, Some(peer_info.addr.0)));
				}
			}

			if local_head.height == 0 && header_height == 0 {
				debug!(
					"sync: no fresh ahead outbound advertisement is available for genesis HeaderSync"
				);
			} else {
				debug!(
					"sync: no outbound peer has demonstrated enough work to leave NoSync"
				);
			}
			return Ok((false, 0, None));
		}

		// Keep a startup request alive until it returns. A short response marks the
		// peer's demonstrated tip and must release the speculative batch target.
		if let SyncStatus::HeaderSync {
			current_height,
			highest_height,
		} = status
		{
			let progress = header_height.saturating_sub(current_height);
			if header_request_pending
				&& highest_height > header_height
				&& progress == 0
				&& highest_height
					<= header_height.saturating_add(p2p::MAX_BLOCK_HEADERS as u64)
			{
				return Ok((true, highest_height, None));
			}
			if highest_height > header_height
				&& progress % p2p::MAX_BLOCK_HEADERS as u64 != 0
			{
				return Ok((true, header_height, None));
			}
		}

		// Once syncing is active, demonstrated progress is authoritative. Raw
		// advertisements may only size a bounded, one-batch-per-peer prefetch window.
		if let Some(peer_info) = validated_outbound_sync_peer(validated_peer_infos, &local_diff) {
			let target = if matches!(status, SyncStatus::HeaderSync { .. }) {
				std::cmp::max(
					header_sync_window_height(
						header_height,
						self.peers
							.outgoing_connected_peers()
							.into_iter()
							.map(|peer| peer.info.clone()),
					),
					bounded_probe_height(header_height, &peer_info),
				)
			} else {
				header_height
			};
			return Ok((true, target, Some(peer_info.addr.0)));
		}
		if matches!(status, SyncStatus::HeaderSync { .. }) && header_height > local_head.height {
			return Ok((true, header_height, None));
		}

		info!(
			"Node synchronized at {} @ {} [{}]",
			local_diff, local_head.height, local_head.last_block_h
		);
		self.sync_state.update(SyncStatus::NoSync);
		Ok((false, header_height, None))
	}

	fn request_header_probe(&self) {
		let header_head = match self.chain.header_head() {
			Ok(tip) => tip,
			Err(_) => return,
		};
		let peer_info = match advertised_outbound_probe_peer(
			self.peers
				.outgoing_connected_peers()
				.into_iter()
				.map(|peer| peer.info.clone()),
			&header_head.total_difficulty,
		) {
			Some(peer) => peer,
			None => return,
		};
		let locator = match get_locator_heights(header_head.height)
			.into_iter()
			.map(|height| {
				self.chain
					.get_header_by_height(height)
					.map(|header| header.hash())
			})
			.collect::<Result<Vec<_>, _>>()
		{
			Ok(locator) => locator,
			Err(e) => {
				debug!("sync: failed to build bounded probe locator: {:?}", e);
				return;
			}
		};
		if let Some(peer) = self.peers.get_connected_peer(peer_info.addr) {
			match peer.send_bounded_header_request(locator) {
				Ok(_) => debug!("sync: sent bounded header probe to {}", peer.info.addr),
				Err(_) => {
					let _ = self.peers.disconnect_peer(peer.info.addr);
				}
			}
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::core::ser::ProtocolVersion;
	use crate::epic::sync::test::TestNode;
	use crate::p2p::{Capabilities, ChainAdapter, Direction, PeerAddr};
	use crate::util::RwLock;

	fn peer_info(
		direction: Direction,
		advertised_difficulty: u64,
		validated_difficulty: Option<u64>,
	) -> PeerInfo {
		let peer = PeerInfo {
			capabilities: Capabilities::UNKNOWN,
			user_agent: "test".to_owned(),
			version: ProtocolVersion::local(),
			addr: PeerAddr("127.0.0.1:3414".parse().unwrap()),
			direction,
			live_info: Arc::new(RwLock::new(p2p::types::PeerLiveInfo::new(
				Difficulty::from_num(advertised_difficulty),
			))),
		};
		if let Some(difficulty) = validated_difficulty {
			peer.update_validated_tip(
				difficulty,
				Difficulty::from_num(difficulty),
				crate::core::core::hash::Hash::from_vec(&[difficulty as u8]),
			);
		}
		peer
	}

	#[test]
	fn peer_wait_resumes_through_initial_reconciliation() {
		let sync_state = SyncState::new();
		sync_state.update(SyncStatus::AwaitingPeers(true));

		resume_sync_after_peer_wait(&sync_state);

		assert_eq!(sync_state.status(), SyncStatus::Initial);

		sync_state.update(SyncStatus::BodySync {
			current_height: 1,
			highest_height: 2,
		});
		resume_sync_after_peer_wait(&sync_state);
		assert_eq!(
			sync_state.status(),
			SyncStatus::BodySync {
				current_height: 1,
				highest_height: 2,
			}
		);
	}

	#[test]
	fn caught_up_initial_state_can_reach_no_sync() {
		let node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		let header = node.chain.header_head().unwrap();
		node.peer.info.update_advertised_tip(
			header.height,
			header.total_difficulty,
			0,
		);
		let sync_state = Arc::new(SyncState::new());
		let runner = SyncRunner::new(
			sync_state.clone(),
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		let (needs_syncing, height, peer) = runner.needs_syncing().unwrap();
		assert!(!needs_syncing);
		assert_eq!(height, 0);
		assert!(peer.is_none());
		assert_eq!(sync_state.status(), SyncStatus::NoSync);
	}

	#[test]
	fn genesis_no_sync_callsite_starts_one_bounded_batch_from_advertised_work() {
		let node = TestNode::with_outbound_peer(Capabilities::UNKNOWN);
		node.peer
			.info
			.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::NoSync);
		let runner = SyncRunner::new(
			sync_state,
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		let (needs_syncing, height, peer) = runner.needs_syncing().unwrap();
		assert!(needs_syncing);
		assert_eq!(height, p2p::MAX_BLOCK_HEADERS as u64);
		assert_eq!(peer, Some(node.peer.info.addr.0));
	}

	#[test]
	fn awaiting_peers_callsite_starts_one_bounded_batch_from_advertised_work() {
		let node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		node.peer
			.info
			.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::AwaitingPeers(true));
		let runner = SyncRunner::new(
			sync_state,
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		let (needs_syncing, height, peer) = runner.needs_syncing().unwrap();
		assert!(needs_syncing);
		assert_eq!(height, p2p::MAX_BLOCK_HEADERS as u64);
		assert_eq!(peer, Some(node.peer.info.addr.0));
	}

	#[test]
	fn awaiting_peers_resumes_from_persisted_header_head_after_restart() {
		let node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		node.persist_header_head(512, 100_000);
		node.peer
			.info
			.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::AwaitingPeers(true));
		let runner = SyncRunner::new(
			sync_state,
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		assert_eq!(node.chain.head().unwrap().height, 0);
		let (needs_syncing, height, peer) = runner.needs_syncing().unwrap();
		assert!(needs_syncing);
		assert_eq!(height, 512 + p2p::MAX_BLOCK_HEADERS as u64);
		assert_eq!(peer, Some(node.peer.info.addr.0));
	}

	#[test]
	fn startup_target_stops_at_advertised_tip() {
		let node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		node.persist_header_head(512, 100_000);
		node.peer
			.info
			.update_advertised_tip(528, Difficulty::from_num(1_000_000), 0);
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::AwaitingPeers(true));
		let runner = SyncRunner::new(
			sync_state,
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		let (needs_syncing, target, selected) = runner.needs_syncing().unwrap();
		assert!(needs_syncing);
		assert_eq!(target, 528);
		assert_eq!(selected, Some(node.peer.info.addr.0));
	}

	#[test]
	fn awaiting_peers_resumes_body_sync_from_persisted_headers() {
		let node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		node.persist_header_head(512, 100_000);
		node.peer
			.info
			.update_advertised_tip(512, Difficulty::from_num(100_000), 0);
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::AwaitingPeers(true));
		let runner = SyncRunner::new(
			sync_state,
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		let (needs_syncing, height, peer) = runner.needs_syncing().unwrap();
		assert!(needs_syncing);
		assert_eq!(height, 512);
		assert!(peer.is_none());
	}

	#[test]
	fn short_header_batch_releases_speculative_target_for_body_sync() {
		let node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		node.persist_header_head(512, 100_000);
		let header = node.chain.get_header_by_height(512).unwrap();
		node.peer.info.update_validated_tip(
			header.height,
			header.total_difficulty(),
			header.hash(),
		);
		node.peer
			.info
			.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::HeaderSync {
			current_height: 500,
			highest_height: 1012,
		});
		let runner = SyncRunner::new(
			sync_state,
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		let (needs_syncing, target, selected) = runner.needs_syncing().unwrap();
		assert!(needs_syncing);
		assert_eq!(target, 512);
		assert!(selected.is_none());
	}

	#[test]
	fn short_leading_batch_cannot_shift_header_window() {
		let header_height = 100;
		let first_start = header_batch_start(header_height, 0);
		let second_start = header_batch_start(header_height, 1);

		assert!(!header_batch_preserves_window(
			first_start,
			second_start,
			1,
		));
		assert!(header_batch_preserves_window(
			first_start,
			second_start,
			p2p::MAX_BLOCK_HEADERS as usize,
		));
		assert!(header_batch_preserves_window(
			first_start,
			first_start + 15,
			16,
		));
	}

	#[test]
	fn full_header_batch_allows_bounded_continuation() {
		let node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		node.persist_header_head(512, 100_000);
		let header = node.chain.get_header_by_height(512).unwrap();
		node.peer.info.update_validated_tip(
			header.height,
			header.total_difficulty(),
			header.hash(),
		);
		node.peer
			.info
			.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::HeaderSync {
			current_height: 0,
			highest_height: 512,
		});
		let runner = SyncRunner::new(
			sync_state,
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		let (needs_syncing, target, selected) = runner.needs_syncing().unwrap();
		assert!(needs_syncing);
		assert_eq!(target, 1024);
		assert_eq!(selected, Some(node.peer.info.addr.0));
	}

	#[test]
	fn no_sync_with_incomplete_local_state_uses_persisted_headers() {
		let node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		node.persist_header_head(512, 100_000);
		node.peer
			.info
			.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::NoSync);
		let runner = SyncRunner::new(
			sync_state.clone(),
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		let (needs_syncing, height, peer) = runner.needs_syncing().unwrap();
		assert!(needs_syncing);
		assert_eq!(height, 512);
		assert!(peer.is_none());
		assert_eq!(sync_state.status(), SyncStatus::NoSync);
	}

	#[test]
	fn txhashset_download_with_incomplete_local_state_survives_provider_loss() {
		let node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		node.persist_header_head(512, 100_000);
		node.peer
			.info
			.update_advertised_tip(512, Difficulty::from_num(100_000), 0);
		let sync_state = Arc::new(SyncState::new());
		let now = chrono::Utc::now();
		let downloading = SyncStatus::TxHashsetDownload {
			start_time: now,
			prev_update_time: now,
			update_time: now,
			prev_downloaded_size: 64,
			downloaded_size: 64,
			total_size: 1_024,
		};
		sync_state.update(downloading);
		let runner = SyncRunner::new(
			sync_state.clone(),
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		let (needs_syncing, height, peer) = runner.needs_syncing().unwrap();
		assert!(needs_syncing);
		assert_eq!(height, 512);
		assert!(peer.is_none());
		assert_eq!(sync_state.status(), downloading);
	}

	#[test]
	fn no_sync_callsite_does_not_enroll_honest_peers_from_an_inbound_lie() {
		let mut node = TestNode::with_outbound_peer(Capabilities::UNKNOWN);
		let honest_outbound = node.peer.clone();
		let liar = node.add_inbound_peer(Capabilities::UNKNOWN);
		let honest_inbound = node.add_inbound_peer(Capabilities::UNKNOWN);
		liar.info
			.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::NoSync);
		let runner = SyncRunner::new(
			sync_state,
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		let (needs_syncing, height, selected) = runner.needs_syncing().unwrap();
		assert!(needs_syncing);
		assert_eq!(height, p2p::MAX_BLOCK_HEADERS as u64);
		assert_eq!(selected, Some(honest_outbound.info.addr.0));
		assert!(!honest_outbound.is_banned());
		assert!(!honest_inbound.is_banned());
	}

	#[test]
	fn no_sync_callsite_has_no_inbound_fallback() {
		let mut node = TestNode::with_outbound_peer(Capabilities::UNKNOWN);
		let inbound = node.add_inbound_peer(Capabilities::UNKNOWN);
		inbound
			.info
			.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		node.server
			.peers
			.disconnect_peer(node.peer.info.addr)
			.unwrap();
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::NoSync);
		let runner = SyncRunner::new(
			sync_state,
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		let (needs_syncing, _, selected) = runner.needs_syncing().unwrap();
		assert!(!needs_syncing);
		assert!(selected.is_none());
		assert!(!inbound.is_banned());
	}

	#[test]
	fn active_sync_callsite_is_not_pinned_by_raw_work() {
		let node = TestNode::with_outbound_peer(Capabilities::UNKNOWN);
		node.peer
			.info
			.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::HeaderSync {
			current_height: 0,
			highest_height: 1_000_000,
		});
		let runner = SyncRunner::new(
			sync_state.clone(),
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		let (needs_syncing, _, selected) = runner.needs_syncing().unwrap();
		assert!(!needs_syncing);
		assert!(selected.is_none());
		assert_eq!(sync_state.status(), SyncStatus::NoSync);
	}

	#[test]
	fn bounded_startup_target_survives_until_validation() {
		let node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		node.peer
			.info
			.update_advertised_tip(0, Difficulty::from_num(1_000_000), 0);
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::AwaitingPeers(true));
		let runner = SyncRunner::new(
			sync_state.clone(),
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		let (needs_syncing, target, selected) = runner.needs_syncing().unwrap();
		assert!(needs_syncing);
		assert_eq!(target, p2p::MAX_BLOCK_HEADERS as u64);
		assert_eq!(selected, Some(node.peer.info.addr.0));
		assert!(header_sync_peer_eligible(
			&node.peer.info,
			0,
			selected,
			false,
			false
		));

		sync_state.update(SyncStatus::HeaderSync {
			current_height: 0,
			highest_height: target,
		});
		node.peer
			.info
			.update_advertised_tip(2_000_000, Difficulty::from_num(2_000_000), 0);
		let (needs_syncing, retained_target, selected) =
			runner.needs_syncing_with_pending(true).unwrap();
		assert!(needs_syncing);
		assert_eq!(retained_target, target);
		assert!(selected.is_none());
	}

	#[test]
	fn completed_startup_request_releases_its_speculative_target() {
		let node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		node.peer
			.info
			.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		node.peer.info.mark_header_probe_started();
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::HeaderSync {
			current_height: 0,
			highest_height: p2p::MAX_BLOCK_HEADERS as u64,
		});
		let runner = SyncRunner::new(
			sync_state.clone(),
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		let (needs_syncing, _, selected) = runner.needs_syncing().unwrap();
		assert!(!needs_syncing);
		assert!(selected.is_none());
		assert_eq!(sync_state.status(), SyncStatus::NoSync);
	}

	#[test]
	fn reconnect_does_not_inherit_validated_work() {
		let mut node = TestNode::with_outbound_peer(Capabilities::UNKNOWN);
		let header = node.chain.get_header_by_height(0).unwrap();
		node.peer.info.update_validated_tip(
			header.height,
			header.total_difficulty(),
			header.hash(),
		);
		let reconnected = node.add_outbound_peer(Capabilities::UNKNOWN);

		assert_eq!(reconnected.info.validated_height(), 0);
		assert_eq!(
			reconnected.info.validated_total_difficulty(),
			Difficulty::zero()
		);
	}

	#[test]
	fn no_sync_transition_preserves_difficulty_threshold() {
		let minimum = Difficulty::from_num(105);
		let boundary = peer_info(Direction::Outbound, 105, Some(105));
		assert!(validated_outbound_sync_peer(vec![boundary], &minimum).is_none());

		let above = peer_info(Direction::Outbound, 106, Some(106));
		assert!(validated_outbound_sync_peer(vec![above], &minimum).is_some());
	}

	#[test]
	fn active_sync_prefetches_one_header_batch_per_fastsync_peer() {
		let peers = (0..3)
			.map(|_| {
				let mut peer = peer_info(Direction::Outbound, 1_000_000, Some(100));
				peer.capabilities = Capabilities::HEADER_FASTSYNC;
				peer.update_advertised_tip(
					1_000_000,
					Difficulty::from_num(1_000_000),
					0,
				);
				peer
			})
			.collect::<Vec<_>>();

		assert_eq!(
			header_sync_window_height(100, peers),
			100 + 3 * p2p::MAX_BLOCK_HEADERS as u64
		);
	}

	#[test]
	fn scheduler_fills_the_lowest_missing_header_batch() {
		let occupied = vec![header_batch_start(100, 0), header_batch_start(100, 2)]
			.into_iter()
			.collect();
		assert_eq!(
			next_header_offset(100, header_batch_start(100, 2), &occupied),
			Some(1)
		);
	}

	#[test]
	fn freed_offset_slot_is_reassigned_after_a_peer_drops() {
		let starts = (0..=3)
			.map(|offset| header_batch_start(100, offset))
			.collect::<Vec<_>>();
		let mut occupied = vec![starts[0], starts[1], starts[3]]
			.into_iter()
			.collect::<HashSet<_>>();

		occupied.remove(&starts[1]);
		assert_eq!(next_header_offset(100, starts[3], &occupied), Some(1));
	}

	#[test]
	fn offset_scheduler_never_requests_beyond_the_network_tip() {
		let first = header_batch_start(100, 0);
		let occupied = HashSet::new();
		assert_eq!(next_header_offset(100, first - 1, &occupied), None);
		assert_eq!(next_header_offset(100, first, &occupied), Some(0));

		let occupied = vec![first].into_iter().collect();
		assert_eq!(
			next_header_offset(100, first + p2p::MAX_BLOCK_HEADERS as u64 - 1, &occupied),
			None
		);
	}

	#[test]
	fn ordered_drain_waits_for_a_missing_batch() {
		let node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		let first = header_batch_start(100, 0);
		let second = header_batch_start(100, 1);
		let mut queue = HashMap::new();
		queue.insert(
			second,
			FastsyncHeaderBatch {
				peer: node.peer.clone(),
				headers: vec![BlockHeader::default()],
			},
		);

		assert!(take_next_header_batch(&mut queue, first).is_none());
		assert!(queue.contains_key(&second));
	}

	#[test]
	fn wrong_offset_batch_is_not_a_bannable_offence() {
		let node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		let expected_height = 1;
		let pending = PendingHeaderSync {
			sync: HeaderSync::new(
				Arc::new(SyncState::new()),
				node.server.peers.clone(),
				node.peer.clone(),
				node.chain.clone(),
				0,
				p2p::MAX_BLOCK_HEADERS as u64,
				0,
			),
			expected_height,
			peer: node.peer.clone(),
		};
		let mut wrong = BlockHeader::default();
		wrong.height = expected_height + 1;
		let mut queue = HashMap::new();

		queue_completed_header_batch(&node.server.peers, pending, vec![wrong], &mut queue);

		assert!(queue.is_empty());
		assert!(!node.peer.is_banned());
		assert!(node
			.server
			.peers
			.get_connected_peer(node.peer.info.addr)
			.is_none());
	}

	#[test]
	fn concurrent_misbehaviour_from_several_peers_bans_nobody() {
		let mut node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		let silent = node.peer.clone();
		let empty = node.add_outbound_peer(Capabilities::HEADER_FASTSYNC);
		let liar = node.add_inbound_peer(Capabilities::HEADER_FASTSYNC);
		for peer in [&silent, &empty, &liar] {
			peer.info.update_advertised_tip(
				1_000_000,
				Difficulty::from_num(1_000_000),
				0,
			);
		}

		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::NoSync);
		let runner = SyncRunner::new(
			sync_state.clone(),
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);
		let (_, _, selected) = runner.needs_syncing().unwrap();
		assert!(selected.is_some());
		assert_ne!(selected, Some(liar.info.addr.0));

		let mut request = HeaderSync::new(
			sync_state,
			node.server.peers.clone(),
			silent.clone(),
			node.chain.clone(),
			0,
			p2p::MAX_BLOCK_HEADERS as u64,
			0,
		);
		request.check_run().unwrap();
		request.expire_request();
		let (_, timed_out) = request.check_run().unwrap();
		assert!(timed_out);

		empty.send_bounded_header_request(vec![]).unwrap();
		assert!(ChainAdapter::headers_received(&*node.server.peers, &[], &empty.info).unwrap());

		assert!(!silent.is_banned());
		assert!(!empty.is_banned());
		assert!(!liar.is_banned());
	}

	#[test]
	fn queued_batch_owner_can_fill_a_gap_without_dropping_ready_work() {
		let peer = peer_info(Direction::Outbound, 1_000_000, Some(100));
		peer.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		peer.mark_header_probe_started();

		assert!(!header_sync_peer_eligible(
			&peer, 100, None, false, false
		));
		assert!(header_sync_peer_eligible(&peer, 100, None, true, false));
	}

	#[test]
	fn pending_request_blocks_a_peer_even_when_it_owns_a_ready_batch() {
		let peer = peer_info(Direction::Outbound, 1_000_000, Some(100));
		peer.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);

		assert!(!header_sync_peer_eligible(&peer, 100, None, true, true));
	}

	#[test]
	fn active_sync_excludes_peers_with_an_outstanding_header_batch() {
		let mut ready = peer_info(Direction::Outbound, 1_000_000, Some(100));
		ready.capabilities = Capabilities::HEADER_FASTSYNC;
		ready.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		let mut pending = peer_info(Direction::Outbound, 1_000_000, Some(100));
		pending.capabilities = Capabilities::HEADER_FASTSYNC;
		pending.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		pending.mark_header_probe_started();

		assert_eq!(
			header_sync_window_height(100, vec![ready, pending]),
			100 + p2p::MAX_BLOCK_HEADERS as u64
		);
	}

	#[test]
	fn active_sync_keeps_single_batch_fallback_for_legacy_peers() {
		let peers = vec![
			peer_info(Direction::Outbound, 1_000_000, Some(100)),
			peer_info(Direction::Outbound, 1_000_000, Some(100)),
		];
		for peer in &peers {
			peer.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		}

		assert_eq!(
			header_sync_window_height(100, peers),
			100 + p2p::MAX_BLOCK_HEADERS as u64
		);
	}

	#[test]
	fn active_sync_does_not_repeat_probe_without_validated_progress() {
		let peer = peer_info(Direction::Outbound, 1_000_000, Some(100));
		peer.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		peer.mark_header_probe_started();

		assert_eq!(bounded_probe_height(100, &peer), 100);
		assert!(advertised_outbound_probe_peer(
			vec![peer],
			&Difficulty::from_num(99),
		)
		.is_none());
	}

	#[test]
	fn validated_progress_advances_with_or_without_advertised_height() {
		let peer = peer_info(Direction::Outbound, 1_000_000, Some(612));
		assert_eq!(peer.advertised_height(), 0);
		assert_eq!(
			bounded_probe_height(612, &peer),
			612 + p2p::MAX_BLOCK_HEADERS as u64
		);

		peer.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		assert_eq!(
			bounded_probe_height(612, &peer),
			612 + p2p::MAX_BLOCK_HEADERS as u64
		);

		let caught_up = peer_info(Direction::Outbound, 612, Some(612));
		assert_eq!(bounded_probe_height(612, &caught_up), 612);
	}

	#[test]
	fn body_sync_target_ignores_unverified_advertised_height() {
		let node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		node.persist_header_head(100, 100_000);
		let header = node.chain.get_header_by_height(100).unwrap();
		node.peer.info.update_validated_tip(
			header.height,
			header.total_difficulty(),
			header.hash(),
		);
		node.peer
			.info
			.update_advertised_tip(1_000_000, Difficulty::from_num(1_000_000), 0);
		let sync_state = Arc::new(SyncState::new());
		sync_state.update(SyncStatus::BodySync {
			current_height: 0,
			highest_height: 100,
		});
		let runner = SyncRunner::new(
			sync_state,
			node.server.peers.clone(),
			node.chain.clone(),
			Arc::new(StopState::new()),
		);

		let (needs_syncing, target, selected) = runner.needs_syncing().unwrap();
		assert!(needs_syncing);
		assert_eq!(target, 100);
		assert_eq!(selected, Some(node.peer.info.addr.0));
	}
}
