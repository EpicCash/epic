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

use std::sync::Arc;

use crate::core::core::BlockHeader;
use chrono::prelude::Utc;

use crate::chain::{self, SyncState, SyncStatus};
use crate::common::types::Error;
use crate::core::core::hash::{Hash, Hashed};
use crate::p2p::{self, Peer, Peers};

//TODO: (Biz) we can reduce this
const HEADER_SYNC_TIMEOUT_SECS: i64 = 15;

pub(super) struct HeaderSync {
	sync_state: Arc<SyncState>,
	peers: Arc<Peers>,
	peer: Arc<Peer>,
	chain: Arc<chain::Chain>,
	history_locator: Vec<(u64, Hash)>,
	header_head_height: u64,
	highest_height: u64,
	syncing_peer: bool,
	offset: u8,
	start_time: i64,
}

impl HeaderSync {
	pub(super) fn new(
		sync_state: Arc<SyncState>,
		peers: Arc<Peers>,
		peer: Arc<Peer>,
		chain: Arc<chain::Chain>,
		header_head_height: u64,
		highest_height: u64,
		offset: u8,
	) -> HeaderSync {
		HeaderSync {
			sync_state,
			peers,
			peer,
			chain,
			history_locator: vec![],
			header_head_height,
			highest_height,
			syncing_peer: false,
			offset,
			start_time: Utc::now().timestamp(),
		}
	}
	pub(super) fn check_run(&mut self) -> Result<(Vec<BlockHeader>, bool), chain::Error> {
		let mut peer_blocks = false;

		match self.peers.get_connected_peer(self.peer.info.addr) {
			Some(peer) => {
				if !Arc::ptr_eq(&peer, &self.peer) || !peer.is_connected() || peer.is_banned() {
					peer_blocks = true;
				}
			}
			None => {
				peer_blocks = true;
			}
		}

		if peer_blocks {
			return Ok((vec![], true));
		}

		if !self.syncing_peer {
			info!(
				"{:?}\tnew sync peer, offset: {:?}",
				self.peer.info.addr, self.offset
			);

			self.sync_state.update(SyncStatus::HeaderSync {
				current_height: self.header_head_height,
				highest_height: self.highest_height,
			});

			self.syncing_peer = true;

			//reset previous queued headers
			self.peer.info.set_headers(vec![]);

			peer_blocks = !self.header_sync();
		} else {
			let headers = self.peer.info.take_headers();
			if !headers.is_empty() {
				return Ok((headers, false));
			}
			peer_blocks = self.header_sync_due();
		}
		Ok((vec![], peer_blocks))
	}

	fn header_sync_due(&mut self) -> bool {
		let now = Utc::now().timestamp();
		if (now - self.start_time) >= HEADER_SYNC_TIMEOUT_SECS {
			debug!("sync: header request to {} timed out", self.peer.info.addr);
			let _ = self.peers.disconnect_peer(self.peer.info.addr);
			return true;
		}

		false
	}

	#[cfg(test)]
	pub(super) fn expire_request(&mut self) {
		self.start_time = Utc::now().timestamp() - HEADER_SYNC_TIMEOUT_SECS;
	}

	fn header_sync(&mut self) -> bool {
		if let Ok(header_head) = self.chain.header_head() {
			let difficulty = header_head.total_difficulty;
			if self.peer.info.advertised_total_difficulty() > difficulty {
				return self.request_headers_fastsync();
			}
		}
		false
	}

	/// Request some block headers from a peer to advance us.
	fn request_headers_fastsync(&mut self) -> bool {
		if let Ok(locator) = self.get_locator() {
			self.start_time = Utc::now().timestamp();

			let request = if self.offset == 0
				&& !self
					.peer
					.info
					.capabilities
					.contains(p2p::types::Capabilities::HEADER_FASTSYNC)
			{
				info!(
					"sync: request slowsync headers: asking {} for headers, {:?}, offset {:?}",
					self.peer.info.addr, locator, self.offset
				);
				self.peer.send_header_request(locator)
			} else {
				info!(
					"sync: request fastsync headers: asking {} for headers, {:?}, offset {:?}",
					self.peer.info.addr, locator, self.offset
				);
				self.peer.send_header_fastsync_request(locator, self.offset)
			};

			match request {
				Ok(_) => {
					self.peer.info.mark_header_probe_started();
					return true;
				}
				Err(e) => {
					debug!(
						"sync: failed to request headers from {}: {:?}",
						self.peer.info.addr, e
					);
					let _ = self.peers.disconnect_peer(self.peer.info.addr);
				}
			}
		}
		false
	}

	/// We build a locator based on sync_head.
	/// Even if sync_head is significantly out of date, we will "reset" it once we
	/// start getting headers back from a peer.
	fn get_locator(&mut self) -> Result<Vec<Hash>, Error> {
		let tip = self.chain.get_sync_head()?;
		let heights = get_locator_heights(tip.height);

		// For security, clear `history_locator[]` in any case of header chain rollback.
		// The easiest way is to check whether the sync head and the header head are identical.
		if self.history_locator.len() > 0 && tip.hash() != self.chain.header_head()?.hash() {
			self.history_locator.retain(|&x| x.0 == 0);
		}

		// For each height we need, we either check if something is close enough from
		// the last locator or go to the database.
		let mut locator: Vec<(u64, Hash)> = vec![(tip.height, tip.last_block_h)];
		for h in heights {
			if let Some(l) = close_enough(&self.history_locator, h) {
				locator.push(l);
			} else {
				// Start at the last known hash and go backward
				let last_loc = locator.last().unwrap().clone();
				let mut header_cursor = self.chain.get_block_header(&last_loc.1);
				while let Ok(header) = header_cursor {
					if header.height == h {
						if header.height != last_loc.0 {
							locator.push((header.height, header.hash()));
						}
						break;
					}
					header_cursor = self.chain.get_header_by_height(h);
				}
			}
		}

		locator.dedup_by(|a, b| a.0 == b.0);
		debug!("sync: locator : {:?}", locator.clone());
		self.history_locator = locator.clone();

		Ok(locator.iter().map(|l| l.1).collect())
	}
}

// Whether we have a value close enough to the provided height in the locator
fn close_enough(locator: &Vec<(u64, Hash)>, height: u64) -> Option<(u64, Hash)> {
	if locator.len() == 0 {
		return None;
	}
	// bounds, lower that last is last
	if locator.last().unwrap().0 >= height {
		return locator.last().map(|l| l.clone());
	}
	// higher than first is first if within an acceptable gap
	if locator[0].0 < height && height.saturating_sub(127) < locator[0].0 {
		return Some(locator[0]);
	}
	for hh in locator.windows(2) {
		if height <= hh[0].0 && height > hh[1].0 {
			if hh[0].0 - height < height - hh[1].0 {
				return Some(hh[0].clone());
			} else {
				return Some(hh[1].clone());
			}
		}
	}
	None
}

// current height back to 0 decreasing in powers of 2
pub(super) fn get_locator_heights(height: u64) -> Vec<u64> {
	let mut current = height;
	let mut heights = vec![];
	while current > 0 {
		heights.push(current);
		if heights.len() >= (p2p::MAX_LOCATORS as usize) - 1 {
			break;
		}
		let next = 2u64.pow(heights.len() as u32);
		current = if current > next { current - next } else { 0 }
	}
	heights.push(0);
	heights
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::core::core::hash;
	use crate::epic::sync::test::TestNode;
	use crate::p2p::{Capabilities, ChainAdapter};

	#[test]
	fn header_timeout_rotates_without_banning_peer() {
		let node = TestNode::with_outbound_peer(Capabilities::UNKNOWN);
		let sync_state = Arc::new(SyncState::new());
		let mut header_sync = HeaderSync::new(
			sync_state,
			node.server.peers.clone(),
			node.peer.clone(),
			node.chain.clone(),
			0,
			1,
			0,
		);
		header_sync.syncing_peer = true;
		header_sync.start_time = Utc::now().timestamp() - HEADER_SYNC_TIMEOUT_SECS;

		let (_, timed_out) = header_sync.check_run().unwrap();
		assert!(timed_out);
		assert!(!node.peer.is_banned());
	}

	#[test]
	fn received_headers_complete_expired_request_without_disconnect() {
		let node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		let mut header_sync = HeaderSync::new(
			Arc::new(SyncState::new()),
			node.server.peers.clone(),
			node.peer.clone(),
			node.chain.clone(),
			0,
			1,
			0,
		);
		header_sync.syncing_peer = true;
		header_sync.start_time = Utc::now().timestamp() - HEADER_SYNC_TIMEOUT_SECS;
		node.peer.info.set_headers(vec![BlockHeader::default()]);

		let (headers, timed_out) = header_sync.check_run().unwrap();
		assert_eq!(headers.len(), 1);
		assert!(!timed_out);
		assert!(node
			.server
			.peers
			.get_connected_peer(node.peer.info.addr)
			.is_some());
	}

	#[test]
	fn issued_header_request_timeout_also_rotates_without_banning() {
		let node = TestNode::with_outbound_peer(Capabilities::UNKNOWN);
		node.peer.info.update_advertised_tip(
			1_000_000,
			crate::core::pow::Difficulty::from_num(1_000_000),
			0,
		);
		let mut header_sync = HeaderSync::new(
			Arc::new(SyncState::new()),
			node.server.peers.clone(),
			node.peer.clone(),
			node.chain.clone(),
			0,
			1,
			0,
		);
		header_sync.check_run().unwrap();
		assert!(!node.peer.info.header_probe_eligible());
		header_sync.start_time = Utc::now().timestamp() - HEADER_SYNC_TIMEOUT_SECS;

		let (_, timed_out) = header_sync.check_run().unwrap();
		assert!(timed_out);
		assert!(!node.peer.is_banned());
		assert!(
			node.server
				.peers
				.get_connected_peer(node.peer.info.addr)
				.is_none()
				|| node.peer.info.header_probe_eligible(),
			"timeout left a connected peer permanently ineligible"
		);
	}

	#[test]
	fn replacement_connection_cannot_complete_an_old_header_request() {
		let mut node = TestNode::with_outbound_peer(Capabilities::HEADER_FASTSYNC);
		let old_peer = node.peer.clone();
		let mut header_sync = HeaderSync::new(
			Arc::new(SyncState::new()),
			node.server.peers.clone(),
			old_peer.clone(),
			node.chain.clone(),
			0,
			1,
			0,
		);
		header_sync.syncing_peer = true;

		let replacement = node.reconnect_outbound_peer();
		assert_eq!(replacement.info.addr, old_peer.info.addr);
		assert!(!Arc::ptr_eq(&replacement, &old_peer));

		let (_, request_finished) = header_sync.check_run().unwrap();
		assert!(request_finished);
		assert!(replacement.is_connected());
	}

	#[test]
	fn empty_headers_are_a_retry_not_a_ban() {
		let node = TestNode::with_outbound_peer(Capabilities::UNKNOWN);
		node.peer.send_bounded_header_request(vec![]).unwrap();
		assert!(node
			.server
			.peers
			.headers_received(&[], &node.peer.info)
			.unwrap());
		assert!(!node.peer.is_banned());
		assert!(node
			.server
			.peers
			.get_connected_peer(node.peer.info.addr)
			.is_none());
	}

	#[test]
	fn test_get_locator_heights() {
		assert_eq!(get_locator_heights(0), vec![0]);
		assert_eq!(get_locator_heights(1), vec![1, 0]);
		assert_eq!(get_locator_heights(2), vec![2, 0]);
		assert_eq!(get_locator_heights(3), vec![3, 1, 0]);
		assert_eq!(get_locator_heights(10), vec![10, 8, 4, 0]);
		assert_eq!(get_locator_heights(100), vec![100, 98, 94, 86, 70, 38, 0]);
		assert_eq!(
			get_locator_heights(1000),
			vec![1000, 998, 994, 986, 970, 938, 874, 746, 490, 0]
		);
		// check the locator is still a manageable length, even for large numbers of
		// headers
		assert_eq!(
			get_locator_heights(10000),
			vec![10000, 9998, 9994, 9986, 9970, 9938, 9874, 9746, 9490, 8978, 7954, 5906, 1810, 0,]
		);
	}

	#[test]
	fn test_close_enough() {
		let zh = hash::ZERO_HASH;

		// empty check
		assert_eq!(close_enough(&vec![], 0), None);

		// just 1 locator in history
		let heights: Vec<u64> = vec![64, 62, 58, 50, 34, 2, 0];
		let history_locator: Vec<(u64, Hash)> = vec![(0, zh.clone())];
		let mut locator: Vec<(u64, Hash)> = vec![];
		for h in heights {
			if let Some(l) = close_enough(&history_locator, h) {
				locator.push(l);
			}
		}
		assert_eq!(locator, vec![(0, zh.clone())]);

		// simple dummy example
		let locator = vec![
			(1000, zh.clone()),
			(500, zh.clone()),
			(250, zh.clone()),
			(125, zh.clone()),
		];
		assert_eq!(close_enough(&locator, 2000), None);
		assert_eq!(close_enough(&locator, 1050), Some((1000, zh)));
		assert_eq!(close_enough(&locator, 900), Some((1000, zh)));
		assert_eq!(close_enough(&locator, 270), Some((250, zh)));
		assert_eq!(close_enough(&locator, 20), Some((125, zh)));
		assert_eq!(close_enough(&locator, 125), Some((125, zh)));
		assert_eq!(close_enough(&locator, 500), Some((500, zh)));

		// more realistic test with 11 history
		let heights: Vec<u64> = vec![
			2554, 2552, 2548, 2540, 2524, 2492, 2428, 2300, 2044, 1532, 508, 0,
		];
		let history_locator: Vec<(u64, Hash)> = vec![
			(2043, zh.clone()),
			(2041, zh.clone()),
			(2037, zh.clone()),
			(2029, zh.clone()),
			(2013, zh.clone()),
			(1981, zh.clone()),
			(1917, zh.clone()),
			(1789, zh.clone()),
			(1532, zh.clone()),
			(1021, zh.clone()),
			(0, zh.clone()),
		];
		let mut locator: Vec<(u64, Hash)> = vec![];
		for h in heights {
			if let Some(l) = close_enough(&history_locator, h) {
				locator.push(l);
			}
		}
		locator.dedup_by(|a, b| a.0 == b.0);
		assert_eq!(
			locator,
			vec![(2043, zh.clone()), (1532, zh.clone()), (0, zh.clone()),]
		);

		// more realistic test with 12 history
		let heights: Vec<u64> = vec![
			4598, 4596, 4592, 4584, 4568, 4536, 4472, 4344, 4088, 3576, 2552, 504, 0,
		];
		let history_locator: Vec<(u64, Hash)> = vec![
			(4087, zh.clone()),
			(4085, zh.clone()),
			(4081, zh.clone()),
			(4073, zh.clone()),
			(4057, zh.clone()),
			(4025, zh.clone()),
			(3961, zh.clone()),
			(3833, zh.clone()),
			(3576, zh.clone()),
			(3065, zh.clone()),
			(1532, zh.clone()),
			(0, zh.clone()),
		];
		let mut locator: Vec<(u64, Hash)> = vec![];
		for h in heights {
			if let Some(l) = close_enough(&history_locator, h) {
				locator.push(l);
			}
		}
		locator.dedup_by(|a, b| a.0 == b.0);
		assert_eq!(
			locator,
			vec![
				(4087, zh.clone()),
				(3576, zh.clone()),
				(3065, zh.clone()),
				(0, zh.clone()),
			]
		);
	}
}
