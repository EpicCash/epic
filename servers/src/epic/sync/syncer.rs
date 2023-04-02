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
use crate::core::global;
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
		// Note: Even if we have skip peer wait we need to wait a
		// short period of time for tests to do the right thing.
		let wait_secs = if let SyncStatus::AwaitingPeers(true) = self.sync_state.status() {
			30
		} else {
			3
		};

		let head = self.chain.head()?;

		let mut n = 0;
		const MIN_PEERS: usize = 3;
		loop {
			if self.stop_state.is_stopped() {
				break;
			}
			let wp = self.peers.more_or_same_work_peers()?;
			// exit loop when:
			// * we have more than MIN_PEERS more_or_same_work peers
			// * we are synced already, e.g. epic was quickly restarted
			// * timeout
			if wp > MIN_PEERS
				|| (wp == 0
					&& self.peers.enough_outbound_peers()
					&& head.total_difficulty > Difficulty::zero())
				|| n > wait_secs
			{
				if wp > 0 || !global::is_production_mode() {
					break;
				}
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
		if let Err(e) = self.wait_for_min_peers() {
			error!("wait_for_min_peers failed: {:?}", e);
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

		let mut waiting_for_queue = false;

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
		let mut highest_height = 0;

		// Main syncing loop
		loop {
			if self.stop_state.is_stopped() {
				//close running header sync threads
				for header_sync in header_syncs {
					let _ = header_sync.1.send(false);
				}
				break;
			}

			thread::sleep(time::Duration::from_millis(10));

			let currently_syncing = self.sync_state.is_syncing();

			// check whether syncing is generally needed, when we compare our state with others
			let (needs_syncing, most_work_height) = unwrap_or_restart_loop!(self.needs_syncing());

			if most_work_height > 0 {
				// we can occasionally get a most work height of 0 if read locks fail
				highest_height = most_work_height;
			}

			//sync_slots = highest_height / sync_slot_size as u64;
			//info!("current sync_slots: {:?}", sync_slots);
			// quick short-circuit (and a decent sleep) if no syncing is needed
			if !needs_syncing {
				if currently_syncing {
					self.sync_state.update(SyncStatus::NoSync);

					// Initial transition out of a "syncing" state and into NoSync.
					// This triggers a chain compaction to keep out local node tidy.
					// Note: Chain compaction runs with an internal threshold
					// so can be safely run even if the node is restarted frequently.
					unwrap_or_restart_loop!(self.chain.compact());
				}

				// sleep for 10 secs but check stop signal every second
				for _ in 1..10 {
					thread::sleep(time::Duration::from_secs(1));
					if self.stop_state.is_stopped() {
						break;
					}
				}
				continue;
			}

			// if syncing is needed
			let head = unwrap_or_restart_loop!(self.chain.head());
			let tail = self.chain.tail().unwrap_or_else(|_| head.clone());
			let header_head = unwrap_or_restart_loop!(self.chain.header_head());
			let mut check_state_sync = false;
			// run each sync stage, each of them deciding whether they're needed
			// except for state sync that only runs if body sync return true (means txhashset is needed)
			//add new header_sync peer if we found a new peer which is not in list

			//add peer to the sync queue. only offset 0 can add old sync peer
			if waiting_for_queue {
				for peer in self.peers.clone().most_work_peers() {
					let peer_addr = peer.info.addr.to_string();
					if (peer
						.info
						.capabilities
						.contains(p2p::types::Capabilities::HEADER_FASTSYNC)
						|| offset == 0) && peer.is_connected()
						&& !peer.is_banned()
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
									header_head.height.clone(),
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
													println!("Terminating sync thread");

													break;
												}
											};

											if stop {
												info!("Sync thread stop");
												break;
											}

											match header_sync.check_run() {
												Ok(headers) => {
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

								if let Ok(mut fastsync_headers) = fastsync_header_queue.try_lock() {
									match fastsync_headers
										.insert(feedback.headers[0].height, feedback)
									{
										Some(_s) => {
											error!("headers already in queue");
										}
										None => {}
									}
								} else {
									error!("failed to get lock to insert headers to queue");
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
				waiting_for_queue = false;

				//end foreach peer
				info!("---------------------- in queue -----------------------");
				if let Ok(fastsync_headers) = fastsync_header_queue.try_lock() {
					let mut sorted: Vec<_> = fastsync_headers.iter().collect();
					sorted.sort_by_key(|a| a.0);
					for (key, value) in sorted.iter() {
						info!(
							"queue item start height: {:?}, headers: {:?}, offset: {:?}",
							key,
							value.headers.len(),
							value.offset
						);
					}
					drop(fastsync_headers);
				}
				info!("---------------------- <------> -----------------------");

				if let Ok(mut fastsync_headers) = fastsync_header_queue.try_lock() {
					//reset if all queue items are processed or get stuck because items in queue can not be added
					if fastsync_headers.len() == 0 || tochain_attemps > 10 {
						waiting_for_queue = true;
						offset = 0;
						tochain_attemps = 0;
						header_syncs = HashMap::new();
						fastsync_headers.clear();
						drop(fastsync_headers);
						continue;
					}

					let current_height = chainsync.adapter.total_header_height().unwrap();
					if let Some(fastsync_header) = fastsync_headers.get(&(current_height + 1)) {
						let headers = fastsync_header.headers.clone();
						let peer_info = fastsync_header.peer_info.clone();

						match chainsync
							.adapter
							.headers_received(&headers.clone(), &peer_info.clone())
						{
							Ok(added) => {
								if !added {
									// if the peer sent us a block header that's intrinsically bad
									// they are either mistaken or malevolent, both of which require a ban

									chainsync
										.ban_peer(
											peer_info.addr,
											p2p::types::ReasonForBan::BadBlockHeader,
										)
										.map_err(|e| {
											let err: chain::Error = chain::ErrorKind::Other(
												format!("ban peer error :{:?}", e),
											)
											.into();
											err
										})
										.unwrap();
								}
								fastsync_headers.remove(&(current_height + 1));
							}
							Err(err) => {
								error!("chainsync {:?}", err);
							}
						}
					} else {
						tochain_attemps += 1;
					}
					//end if fastsync_header
				}
			}

			match self.sync_state.status() {
				SyncStatus::TxHashsetDownload { .. }
				| SyncStatus::TxHashsetSetup
				| SyncStatus::TxHashsetRangeProofsValidation { .. }
				| SyncStatus::TxHashsetKernelsValidation { .. }
				| SyncStatus::TxHashsetSave
				| SyncStatus::TxHashsetDone => check_state_sync = true,
				SyncStatus::NoSync | SyncStatus::Initial => {
					let sync_head = self.chain.get_sync_head().unwrap();
					debug!(
                        "sync: initial transition to HeaderSync. sync_head: {} at {}, resetting to: {} at {}",
                        sync_head.hash(),
                        sync_head.height,
                        header_head.hash(),
                        header_head.height,
                    );

					// Reset sync_head to header_head on transition to HeaderSync,
					// but ONLY on initial transition to HeaderSync state.
					//
					// The header_head and sync_head may diverge here in the presence of a fork
					// in the header chain. Ensure we track the new advertised header chain here
					// correctly, so reset any previous (and potentially stale) sync_head to match
					// our last known "good" header_head.
					//
					let _ = self.chain.reset_sync_head();
					// Rebuild the sync MMR to match our updated sync_head.
					let _ = self.chain.rebuild_sync_mmr(&header_head);
					waiting_for_queue = true;
				}
				_ => {
					// skip body sync if header chain is not synced.
					if header_head.height < highest_height {
						continue;
					}

					let check_run = match body_sync.check_run(&head, highest_height) {
						Ok(v) => v,
						Err(e) => {
							error!("check_run failed: {:?}", e);
							continue;
						}
					};

					if check_run {
						check_state_sync = true;
					}
				}
			}

			if check_state_sync {
				state_sync.check_run(&header_head, &head, &tail, highest_height);
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
