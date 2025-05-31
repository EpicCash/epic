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

use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time;
use std::{thread, thread::JoinHandle};

use crate::chain::{self, SyncState, SyncStatus};
use crate::core::core::hash::Hashed;

use crate::core::pow::Difficulty;
use crate::epic::sync::body_sync::BodySync;
use crate::epic::sync::header_sync::HeaderSync;
use crate::epic::sync::state_sync::StateSync;
use crate::p2p;
use crate::util::StopState;

// all for FastsyncHeaderQueue
use crate::core::core::BlockHeader;
use crate::p2p::PeerInfo;
use std::collections::HashMap;

#[derive(Clone)]
pub struct FastsyncHeaderQueue {
	offset: u8,
	peer_info: PeerInfo,
	headers: Vec<BlockHeader>,
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

pub struct SyncRunner {
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
				info!("Sufficient outbound peers connected, proceeding with sync.");
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
			}
			Err(e) => {
				error!("wait_for_min_peers failed: {:?}", e);
				// If the minimum peers requirement is not met, log and restart the loop
				return; // Beende die aktuelle Iteration der Schleife
			}
		}

		// Our 3 main sync stages
		// fast header sync
		//let mut header_syncs: HashMap<String, Rc<RefCell<HeaderSync>>> = HashMap::new();
		let mut header_syncs: HashMap<String, std::sync::mpsc::Sender<bool>> = HashMap::new();
		let mut offset = 0;
		let mut tochain_attemps = 0;

		let fastsync_header_queue: Arc<std::sync::Mutex<HashMap<u64, FastsyncHeaderQueue>>> =
			Arc::new(std::sync::Mutex::new(HashMap::new()));

		let chainsync = self.peers.clone();

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
		let mut first_sync_loop = true;
		// Main syncing loop
		loop {
			// Check if the node is stopped
			if self.stop_state.is_stopped() {
				// Close running header sync threads
				for header_sync in header_syncs {
					let _ = header_sync.1.send(false);
				}
				break;
			}

			// Check if there are enough outbound peers
			if !self.peers.enough_outbound_peers() {
				warn!("Not enough outbound peers available. Waiting for more peers to connect...");
				self.sync_state.update(SyncStatus::AwaitingPeers(false));
				thread::sleep(time::Duration::from_secs(5));
				continue; // Skip the current iteration of the loop
			}

			let currently_syncing = self.sync_state.is_syncing();

			// Check whether syncing is generally needed by comparing our state with others
			let (mut needs_syncing, mut most_work_height) =
				unwrap_or_restart_loop!(self.needs_syncing());

			// On the very first sync loop after startup, force syncing to ensure we check for new data
			// even if our local state appears up-to-date. This guarantees the node always attempts
			// to synchronize with the network at startup.
			if first_sync_loop {
				needs_syncing = true;
				if let Some(peer) = self.peers.most_work_peer() {
					most_work_height = peer.info.height();
				}
				first_sync_loop = false;
			}

			if most_work_height > 0 {
				// Occasionally, we can get a most work height of 0 if read locks fail
				highest_network_height = most_work_height;
			}
			let sleep_duration = match self.sync_state.status() {
				SyncStatus::HeaderSync { .. } | SyncStatus::Initial => {
					time::Duration::from_millis(100)
				}
				SyncStatus::BodySync { .. } => time::Duration::from_millis(100),
				_ => time::Duration::from_secs(10),
			};

			// Quick short-circuit (and a decent sleep) if no syncing is needed
			let mut needs_headersync = false;
			if !needs_syncing {
				if currently_syncing {
					// Transition out of a "syncing" state and into NoSync
					self.sync_state.update(SyncStatus::Compacting);
					// Initial transition out of a "syncing" state and into NoSync.
					// This triggers a chain compaction to keep our local node tidy.
					// Note: Chain compaction runs with an internal threshold
					// so it can be safely run even if the node is restarted frequently.
					unwrap_or_restart_loop!(self.chain.compact());
					self.sync_state.update(SyncStatus::NoSync);
				}

				// Sleep for 10 seconds but check the stop signal every second
				for _ in 1..10 {
					thread::sleep(time::Duration::from_secs(1));
					if self.stop_state.is_stopped() {
						break;
					}
				}
				continue;
			} else if self.sync_state.status() == SyncStatus::NoSync {
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

			if download_headers {
				for peer in self.peers.clone().most_work_peers() {
					let peer_addr = peer.info.addr.to_string();
					if (peer
						.info
						.capabilities
						.contains(p2p::types::Capabilities::HEADER_FASTSYNC)
						|| offset == 0) && peer.is_connected()
						&& !peer.is_banned()
						&& (header_head.height + (offset as u64 * 512)) < highest_network_height
					{
						let mut remove_peer_from_sync = false;
						match header_syncs.get(&peer_addr) {
							Some(header_sync) => {
								if let Err(_e) = header_sync.send(false) {
									remove_peer_from_sync = true;
								}
							}

							None => {
								let (sender, receiver) = channel();

								let mut header_sync = HeaderSync::new(
									self.sync_state.clone(),
									self.peers.clone(),
									peer.clone(),
									self.chain.clone(),
									header_head.height.clone(),
									highest_network_height.clone(),
									offset.clone(),
								);

								let handler: JoinHandle<FastsyncHeaderQueue> =
									thread::spawn(move || {
										let mut synchthread_headers = FastsyncHeaderQueue {
											offset: header_sync.offset(),
											peer_info: peer.info.clone(),
											headers: vec![],
										};
										loop {
											let stop = match receiver.try_recv() {
												Ok(rcv) => rcv,
												Err(std::sync::mpsc::TryRecvError::Empty) => false,
												Err(
													std::sync::mpsc::TryRecvError::Disconnected,
												) => {
													debug!("Terminating sync thread");

													break;
												}
											};

											if stop {
												debug!("Sync header thread stopped");
												break;
											}

											match header_sync.check_run() {
												Ok((headers, peer_blocks)) => {
													if peer_blocks {
														break;
													}
													if headers.len() > 0 {
														synchthread_headers.headers = headers;
														break;
													}
												}
												Err(_) => break,
											}

											thread::sleep(time::Duration::from_millis(1000));
										}
										synchthread_headers
									});

								let feedback = handler.join().unwrap();
								// Don't process if headers are empty
								if feedback.headers.len() <= 0 {
									continue;
								}

								if let Ok(mut fastsync_headers) = fastsync_header_queue.try_lock() {
									match fastsync_headers
										.insert(feedback.headers[0].height, feedback)
									{
										Some(_s) => {
											error!("Headers already in queue");
										}
										None => {}
									}
								} else {
									error!("Failed to get lock to insert headers to queue");
								}

								offset = offset + 1 as u8;
								header_syncs.insert(peer_addr.clone(), sender);
							}
						}

						if remove_peer_from_sync {
							header_syncs.remove(&peer_addr);
						}
					}
				}
			}

			if header_syncs.len() > 0 {
				download_headers = false;

				// Just for stats
				if let Ok(mut fastsync_headers) = fastsync_header_queue.try_lock() {
					info!("------------ Downloaded headers in queue ------------");

					let mut sorted: Vec<_> =
						fastsync_headers.clone().into_iter().collect::<Vec<_>>();
					sorted.sort_by_key(|a| a.0);

					for (key, value) in sorted.iter() {
						info!(
							"Start height: {:?}, Headers: {:?}, offset: {:?}",
							key,
							value.headers.len(),
							value.offset
						);
					}
					info!("------------------ <-------------> ------------------");

					// Reset if all queue items are processed or get stuck because items in queue cannot be added
					if fastsync_headers.len() == 0 || tochain_attemps > 10 {
						download_headers = true;
						offset = 0;
						tochain_attemps = 0;
						header_syncs = HashMap::new();
						fastsync_headers.clear();
						drop(fastsync_headers);
						continue;
					}

					let lowest_height = sorted.iter().next().clone().unwrap().0;
					let fastsync_header = sorted.get(0);
					let headers = &fastsync_header.unwrap().1.headers;
					let peer_info = &fastsync_header.unwrap().1.peer_info;

					match chainsync
						.adapter
						.headers_received(&headers.clone(), &peer_info.clone())
					{
						Ok(added) => {
							if !added {
								// If the peer sent us a block header that's intrinsically bad
								// they are either mistaken or malevolent, both of which require a ban
								chainsync
									.ban_peer(
										peer_info.addr,
										p2p::types::ReasonForBan::BadBlockHeader,
									)
									.map_err(|e| {
										let err: chain::Error =
											chain::Error::Other(format!("Ban peer error :{:?}", e))
												.into();
										err
									})
									.unwrap();
							}
							fastsync_headers.remove(&lowest_height);
						}
						Err(err) => {
							error!("Chainsync {:?}", err);
						}
					}
				} else {
					tochain_attemps += 1;
				} // End if fastsync_header
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
					let (_needs_syncing, most_work_height) =
						unwrap_or_restart_loop!(self.needs_syncing());

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
					let sync_head = self.chain.get_sync_head().unwrap();
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
					// Asking peers for headers and start header sync tasks
					download_headers = false;

					self.sync_state.update(SyncStatus::BodySync {
						current_height: head.height,
						highest_height: highest_network_height,
					});

					continue;
				}

				SyncStatus::AwaitingPeers(_) => {
					// Apply only on startup
					if !download_headers {
						let sync_head = self.chain.get_sync_head().unwrap();
						info!(
								"Initial transition to HeaderSync. Head {} at {}, resetting to: {} at {}",
								sync_head.hash(),
								sync_head.height,
								header_head.hash(),
								header_head.height,
							);

						let _ = self.chain.reset_sync_head();
						// Rebuild the sync MMR to match our updated sync_head.
						let _ = self.chain.rebuild_sync_mmr(&header_head);
						// Asking peers for headers and start header sync tasks
						download_headers = true;
						//continue;
					}
				}

				_ => {
					if header_head.height >= highest_network_height {
						// Header-Synchronisierung abgeschlossen

						for header_sync in header_syncs.clone() {
							let _ = header_sync.1.send(false);
						}

						// Wechsel zu Body-Synchronisierung
						self.sync_state.update(SyncStatus::BodySync {
							current_height: head.height,
							highest_height: highest_network_height,
						});
						download_headers = false;

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
						// Only start a new header download if the queue is empty and no header syncs are running
						let queue_empty = fastsync_header_queue
							.try_lock()
							.map(|q| q.is_empty())
							.unwrap_or(false);
						if queue_empty && header_syncs.is_empty() {
							download_headers = true;
						} else {
							download_headers = false;
						}

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
	fn needs_syncing(&self) -> Result<(bool, u64), chain::Error> {
		let local_diff = self.chain.head()?.total_difficulty;
		let mut is_syncing = self.sync_state.is_syncing();
		let peer = self.peers.most_work_peer();

		let peer_info = if let Some(p) = peer {
			p.info.clone()
		} else {
			warn!("No peers available, can not sync");
			return Ok((false, 0));
		};

		// if we're already syncing, we're caught up if no peer has a higher
		// difficulty than us
		if is_syncing {
			if peer_info.total_difficulty() <= local_diff {
				let ch = self.chain.head()?;
				info!(
					"Node synchronized at {} @ {} [{}]",
					local_diff, ch.height, ch.last_block_h
				);
				is_syncing = false;
			}
		} else {
			// sum the last 5 difficulties to give us the threshold
			let threshold = {
				let diff_iter = match self.chain.difficulty_iter() {
					Ok(v) => v,
					Err(e) => {
						error!("failed to get difficulty iterator: {:?}", e);
						// we handle 0 height in the caller
						return Ok((false, 0));
					}
				};
				diff_iter
					.map(|x| x.difficulty)
					.take(5)
					.fold(Difficulty::zero(), |sum, val| sum + val)
			};

			let peer_diff = peer_info.total_difficulty();
			if peer_diff > local_diff.clone() + threshold.clone() {
				debug!(
					"sync: total_difficulty {}, peer_difficulty {}, threshold {} (last 5 blocks), enabling sync",
					local_diff,
					peer_diff,
					threshold,
				);
				is_syncing = true;
			}
		}
		Ok((is_syncing, peer_info.height()))
	}
}
