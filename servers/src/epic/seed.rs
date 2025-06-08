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

//! Seeds a server with initial peers on first start and keep monitoring
//! peer counts to connect to more if neeed. Seedin strategy is
//! configurable with either no peers, a user-defined list or a preset
//! list of DNS records (the default).

use std::collections::HashMap;
use std::collections::HashSet;
use std::net::ToSocketAddrs;
use std::sync::{mpsc, Arc};
use std::{cmp, str, thread, time};

use chrono::{DateTime, Utc};
use chrono::{Duration, NaiveDate};
use rand::rng;
use rand::seq::SliceRandom;

use crate::core::global;
use crate::p2p;
use crate::p2p::types::PeerAddr;
use crate::p2p::ChainAdapter;
use crate::util::StopState;

// DNS Seeds with contact email associated
const MAINNET_DNS_SEEDS: &'static [&'static str] = &[
	"ec2-54-233-177-64.sa-east-1.compute.amazonaws.com",
	"ec2-3-218-126-145.compute-1.amazonaws.com",
];
const FLOONET_DNS_SEEDS: &'static [&'static str] = &["95.217.197.180"];
pub fn connect_and_monitor(
	p2p_server: Arc<p2p::Server>,
	capabilities: p2p::Capabilities,
	seed_list: Box<dyn Fn() -> Vec<PeerAddr> + Send>,
	preferred_peers: Option<Vec<PeerAddr>>,
	stop_state: Arc<StopState>,
) -> std::io::Result<thread::JoinHandle<()>> {
	thread::Builder::new()
		.name("seed".to_string())
		.spawn(move || {
			let peers = p2p_server.peers.clone();
			let sl = seed_list();

			// open a channel with a listener that connects every peer address sent below
			let (tx, rx) = mpsc::channel();

			let mut prev = NaiveDate::MIN.and_hms_opt(0, 0, 0).unwrap();
			let mut prev_expire_check = NaiveDate::MIN.and_hms_opt(0, 0, 0).unwrap();
			let mut prev_ping = Utc::now().naive_utc();
			let mut start_attempt = 0;
			let mut connecting_history: HashMap<PeerAddr, DateTime<Utc>> = HashMap::new();
			let mut prev_peer_request = Utc::now().naive_utc();
			//prepare all peers
			for mut peer in peers.all_peers() {
				// Unban peer if it was banned with no reason
				// This is a workaround for the case when a peer is accidentally banned
				if matches!(peer.ban_reason, p2p::ReasonForBan::None)
					&& peer.flags == p2p::State::Banned
				{
					debug!("Unbanning peer {} with no ban reason", peer.addr);
					let _ = peers.unban_peer(peer.addr);
				}

				// Ingore last connected time on startup!
				peer.last_connected = 0;
				let _ = peers.save_peer(&peer);
			}

			//try to immidiately connect to know healthy if we have some in store
			connect_to_healthy_peers(tx.clone(), peers.clone());

			loop {
				if stop_state.is_stopped() {
					break;
				}

				// Pause egress peer connection request. Only for tests.
				if stop_state.is_paused() {
					thread::sleep(time::Duration::from_secs(1));
					continue;
				}

				// ask for new peer list from connected peers
				if Utc::now().naive_utc() - prev_peer_request > Duration::minutes(5) {
					for peer in peers.all_peers() {
						if peer.flags == p2p::State::Healthy && peers.is_connected(peer.addr) {
							if let Some(conn) = peers.get_connected_peer(peer.addr) {
								let _ = conn.send_peerlist_request(capabilities);
							}
						}
					}
					prev_peer_request = Utc::now().naive_utc();
				}

				// Check for and remove expired peers from the storage
				if Utc::now().naive_utc() - prev_expire_check > Duration::hours(1) {
					peers.remove_expired_defunc_peers();

					prev_expire_check = Utc::now().naive_utc();
				}

				// Try to connect to the remote seeds
				// This helps when the remote seed servers are down during startup
				if peers.peer_count() == 0 {
					connect_to_seeds_and_preferred_peers(
						tx.clone(),
						sl.clone(),
						preferred_peers.clone(),
					);
					start_attempt = 0; // reset start attempt after connecting to seeds
				}

				// set 10 random peers that have state defunc to unknown
				promote_defunct_to_unknown(&peers);

				// make several attempts to get peers as quick as possible
				// with exponential backoff
				if Utc::now().naive_utc() - prev
					> Duration::seconds(cmp::min(20, 1 << start_attempt))
				{
					// try to connect to any address sent to the channel
					listen_for_addrs(
						peers.clone(),
						p2p_server.clone(),
						capabilities,
						&rx,
						&mut connecting_history,
					);

					// monitor additional peers if we need to add more
					monitor_peers(peers.clone(), p2p_server.config.clone(), tx.clone());

					prev = Utc::now().naive_utc();
					start_attempt = cmp::min(6, start_attempt + 1);
				}

				// Ping connected peers on every 10s to monitor peers.
				if Utc::now().naive_utc() - prev_ping > Duration::seconds(10) {
					let total_diff = peers.total_difficulty();
					let total_height = peers.total_height();
					if total_diff.is_ok() && total_height.is_ok() {
						peers.check_all(total_diff.unwrap(), total_height.unwrap());
						prev_ping = Utc::now().naive_utc();
					} else {
						error!("failed to get peers difficulty and/or height");
					}
				}

				thread::sleep(time::Duration::from_secs(1));
			}
		})
}

fn monitor_peers(peers: Arc<p2p::Peers>, config: p2p::P2PConfig, tx: mpsc::Sender<PeerAddr>) {
	let total_count = peers.all_peers().len();
	let mut healthy_count = 0;
	let mut banned_count = 0;
	let mut defuncts = vec![];
	let mut unknown = vec![];
	let mut healthy = vec![];

	for x in peers.all_peers() {
		match x.flags {
			p2p::State::Banned => {
				let interval = Utc::now().timestamp() - x.last_banned;
				// Unban peer
				if interval >= config.ban_window() {
					if let Err(e) = peers.unban_peer(x.addr) {
						error!("failed to unban peer {}: {:?}", x.addr, e);
					}
					debug!(
						"monitor_peers: unbanned {} after {} seconds",
						x.addr, interval
					);
				} else {
					banned_count += 1;
				}
			}
			p2p::State::Healthy => {
				healthy_count += 1;
				healthy.push(x);
			}
			p2p::State::Defunct => defuncts.push(x),
			p2p::State::Unknown => unknown.push(x),
		}
	}

	info!(
		"Monitor peers on {}:{}, [inbound/outbound/all] {}/{}/{} connected ({} on tip). \
		 all {} = {} healthy + {} banned + {} defunct",
		config.host,
		config.port,
		peers.peer_inbound_count(),    // Number of inbound connections
		peers.peer_outbound_count(),   // Number of outbound connections
		peers.peer_count(),            // Total number of connected peers
		peers.most_work_peers().len(), // Number of peers with the highest work
		total_count,                   // Total number of known peers
		healthy_count,                 // Number of healthy peers
		banned_count,                  // Number of banned peers
		defuncts.len(),                // Number of defunct (connecting failed) peers
	);

	// Clean up peers as before
	peers.clean_peers(
		config.peer_max_inbound_count() as usize,
		config.peer_max_outbound_count() as usize,
	);

	let max_outbound = config.peer_max_outbound_count() as usize;
	let outbound_count = peers.peer_outbound_count() as usize;

	// --- NEW: Periodically drop and replace an outbound peer if at max ---
	if outbound_count >= max_outbound {
		// Drop the oldest or a random outbound peer (except protected ones)
		if let Some(peer_to_drop) = peers.random_outbound_peer() {
			debug!(
				"Dropping outbound peer {} to allow rotation",
				peer_to_drop.info.addr
			);
			let _ = peers.disconnect_peer(peer_to_drop.info.addr);
		}
	}

	let new_peers_limit = 10;
	let mut new_peers =
		peers.find_peers(p2p::State::Unknown, p2p::Capabilities::UNKNOWN, usize::MAX);
	new_peers.shuffle(&mut rng());
	let new_peers: Vec<_> = new_peers.into_iter().take(new_peers_limit).collect();

	// Send all combined peers to the connection queue
	for p in new_peers {
		if let Ok(false) = peers.is_known(p.addr) {
			trace!("try sending peer addr to connection queue: {}", p.addr);
			let _ = tx.send(p.addr);
		}
	}
}

/// Set 10 random peers with state Defunct to Unknown,
/// to discover them again later.
fn promote_defunct_to_unknown(peers: &Arc<p2p::Peers>) {
	let mut defunct_peers: Vec<_> = peers
		.all_peers()
		.into_iter()
		.filter(|peer| peer.flags == p2p::State::Defunct)
		.collect();

	defunct_peers.shuffle(&mut rng());

	for mut peer in defunct_peers.into_iter().take(10) {
		peer.flags = p2p::State::Unknown;
		let _ = peers.save_peer(&peer);
	}
}

/// Connect to all healthy peers from the peer store.
fn connect_to_healthy_peers(tx: mpsc::Sender<PeerAddr>, peers: Arc<p2p::Peers>) {
	let healthy_peers: Vec<PeerAddr> = peers
		.all_peers()
		.into_iter()
		.filter(|peer| peer.flags == p2p::State::Healthy)
		.map(|peer| peer.addr)
		.collect();

	if healthy_peers.is_empty() {
		warn!("No healthy peers found in store.");
	}

	for addr in healthy_peers {
		info!("Connecting to healthy peer address: {}", addr);
		if let Err(e) = tx.send(addr) {
			error!(
				"Failed to send healthy peer addr {} to connection queue: {:?}",
				addr, e
			);
		}
	}
}

// Check if we have any pre-existing peer in db. If so, start with those,
// otherwise use the seeds provided.
fn connect_to_seeds_and_preferred_peers(
	tx: mpsc::Sender<PeerAddr>,
	seed_list: Vec<PeerAddr>,
	peers_preferred_list: Option<Vec<PeerAddr>>,
) {
	// Start with the seed list
	let mut peer_addrs = seed_list;

	// If we have preferred peers, add them to the list
	if let Some(mut preferred) = peers_preferred_list {
		peer_addrs.append(&mut preferred);
	} else {
		trace!("No preferred peers");
	}

	if peer_addrs.is_empty() {
		warn!("No seeds or preferred peers were provided.");
	}

	// Connect to each address in the combined list
	for addr in peer_addrs {
		info!("Connecting to seed and preferred peers address: {}", addr);
		if let Err(e) = tx.send(addr) {
			error!(
				"Failed to send peer addr {} to connection queue: {:?}",
				addr, e
			);
		}
	}
}

/// Regularly poll a channel receiver for new addresses and initiate a
/// connection if the max peer count isn't exceeded. A request for more
/// peers is also automatically sent after connection.
fn listen_for_addrs(
	peers: Arc<p2p::Peers>,
	p2p: Arc<p2p::Server>,
	capab: p2p::Capabilities,
	rx: &mpsc::Receiver<PeerAddr>,
	connecting_history: &mut HashMap<PeerAddr, DateTime<Utc>>,
) {
	// If we have a healthy number of outbound peers then we are done here.
	let max_inbound = p2p.config.peer_max_inbound_count() as usize;
	let max_outbound = p2p.config.peer_max_outbound_count() as usize;
	if peers.peer_inbound_count() as usize >= max_inbound
		|| peers.peer_outbound_count() as usize >= max_outbound
	{
		return;
	}

	// Pull everything currently on the queue off the queue.
	// Does not block so addrs may be empty.
	// We will take(max_peers) from this later but we want to drain the rx queue
	// here to prevent it backing up.
	let mut seen = HashSet::new();
	let addrs: Vec<PeerAddr> = rx.try_iter().filter(|addr| seen.insert(*addr)).collect();

	// Note: We drained the rx queue earlier to keep it under control.
	// Even if there are many addresses to try we will only try a bounded number of them for safety.
	let connect_min_interval = 30;
	let max_peers_to_connect = 128;
	let startup_mode = connecting_history.is_empty();
	for addr in addrs.into_iter().take(max_peers_to_connect) {
		if peers.is_connected(addr) {
			debug!("peer_connect: already connected to {}", addr);
			continue;
		}

		// ignore the duplicate connecting to same peer within 30 seconds
		let now = Utc::now();

		if !startup_mode {
			if let Some(last_connect_time) = connecting_history.get(&addr) {
				if *last_connect_time + Duration::seconds(connect_min_interval) > now {
					debug!(
						"peer_connect: ignore a duplicate request to {}. previous connecting time: {}",
						addr,
						last_connect_time.format("%H:%M:%S%.3f").to_string(),
					);
					continue;
				}
			}
		}

		connecting_history.insert(addr, now);

		let peers_c = peers.clone();
		let p2p_c = p2p.clone();
		thread::Builder::new()
			.name("peer_connect".to_string())
			.spawn(move || match p2p_c.connect(addr) {
				Ok(p) => {
					if p.send_peerlist_request(capab).is_ok() {
						let _ = peers_c.update_state(addr, p2p::State::Healthy);
					}

					if let Some(mut peer) = peers_c.get_peer(addr).ok() {
						peer.last_connected = Utc::now().timestamp();
						let _ = peers_c.save_peer(&peer);
					}
				}
				Err(_) => {
					if let Some(mut peer) = peers_c.get_peer(addr).ok() {
						peer.last_connected = Utc::now().timestamp();
						let _ = peers_c.save_peer(&peer);
					}
					let _ = peers_c.update_state(addr, p2p::State::Defunct);
				}
			})
			.expect("failed to launch peer_connect thread");
	}

	// shrink the connecting history.
	// put a threshold here to avoid frequent shrinking in every call
	if connecting_history.len() > 100 {
		let now = Utc::now();
		let old: Vec<_> = connecting_history
			.iter()
			.filter(|&(_, t)| *t + Duration::seconds(connect_min_interval) < now)
			.map(|(s, _)| s.clone())
			.collect();
		for addr in old {
			connecting_history.remove(&addr);
		}
	}
}

pub fn dns_seeds() -> Box<dyn Fn() -> Vec<PeerAddr> + Send> {
	Box::new(|| {
		let mut addresses: Vec<PeerAddr> = vec![];
		let net_seeds = if global::is_floonet() {
			FLOONET_DNS_SEEDS
		} else {
			MAINNET_DNS_SEEDS
		};
		for dns_seed in net_seeds {
			let temp_addresses = addresses.clone();
			debug!("Retrieving seed nodes from dns {}", dns_seed);
			match (dns_seed.to_owned(), 0).to_socket_addrs() {
				Ok(addrs) => addresses.append(
					&mut (addrs
						.map(|mut addr| {
							addr.set_port(if global::is_floonet() { 13414 } else { 3414 });
							PeerAddr(addr)
						})
						.filter(|addr| !temp_addresses.contains(addr))
						.collect()),
				),
				Err(e) => debug!("Failed to resolve seed {:?} got error {:?}", dns_seed, e),
			}
		}
		debug!("Retrieved seed addresses: {:?}", addresses);
		addresses
	})
}

/// Convenience function when the seed list is immediately known. Mostly used
/// for tests.
pub fn predefined_seeds(addrs: Vec<PeerAddr>) -> Box<dyn Fn() -> Vec<PeerAddr> + Send> {
	Box::new(move || addrs.clone())
}
