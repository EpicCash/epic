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

use chrono::prelude::{DateTime, Utc};
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

			// Remove all localhost peers (127.0.0.*) before starting peer management
			peers.remove_localhost_peers();

			// open a channel with a listener that connects every peer address sent below
			// max peer count
			let (tx, rx) = mpsc::channel();

			let mut prev = NaiveDate::MIN.and_hms_opt(0, 0, 0).unwrap();
			let mut prev_expire_check = NaiveDate::MIN.and_hms_opt(0, 0, 0).unwrap();
			let mut prev_seed_check = NaiveDate::MIN.and_hms_opt(0, 0, 0).unwrap();
			let mut prev_ping = Utc::now().naive_utc();
			let mut start_attempt = 0;
			let mut connecting_history: HashMap<PeerAddr, DateTime<Utc>> = HashMap::new();

			// Reset all Defunct peers we connected to (if capabilities not empty) Healthy on startup
			for peer in peers.all_peers() {
				if peer.flags == p2p::State::Defunct
					&& peer.capabilities != p2p::Capabilities::UNKNOWN
				{
					debug!("Resetting defunct peer {} to healthy on startup", peer.addr);
					let _ = peers.update_state(peer.addr, p2p::State::Healthy);
				}

				// Unban peer if it was banned with no reason
				// This is a workaround for the case when a peer was banned
				if matches!(peer.ban_reason, p2p::ReasonForBan::None)
					&& peer.flags == p2p::State::Banned
				{
					debug!("Unbanning peer {} with no ban reason", peer.addr);
					let _ = peers.unban_peer(peer.addr);
				}
			}

			loop {
				if stop_state.is_stopped() {
					break;
				}

				// Pause egress peer connection request. Only for tests.
				if stop_state.is_paused() {
					thread::sleep(time::Duration::from_secs(1));
					continue;
				}

				// Check for and remove expired peers from the storage
				if Utc::now().naive_utc() - prev_expire_check > Duration::hours(1) {
					peers.remove_expired();

					prev_expire_check = Utc::now().naive_utc();
				}

				// Try to connect to the remote seeds
				// This helps when the remote seed servers are down during startup
				if !peers.enough_outbound_peers()
					&& Utc::now().naive_utc() - prev_seed_check > Duration::seconds(10)
				{
					connect_to_seeds_and_preferred_peers(
						peers.clone(),
						tx.clone(),
						sl.clone(),
						preferred_peers.clone(),
					);
					prev_seed_check = Utc::now().naive_utc();
				}

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
					monitor_peers(
						peers.clone(),
						p2p_server.config.clone(),
						tx.clone(),
						preferred_peers.clone(),
					);

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

fn monitor_peers(
	peers: Arc<p2p::Peers>,
	config: p2p::P2PConfig,
	tx: mpsc::Sender<PeerAddr>,
	preferred_peers_list: Option<Vec<PeerAddr>>,
) {
	let total_count = peers.all_peers().len();
	let mut healthy_count = 0;
	let mut banned_count = 0;
	let mut defuncts = vec![];

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
			p2p::State::Healthy => healthy_count += 1,
			p2p::State::Defunct => defuncts.push(x),
		}
	}

	info!(
		"Monitor peers on {}:{}, [inbound/outbound/all] {}/{}/{} connected ({} on tip). \
		 all {} = {} healthy + {} banned + {} defunct",
		config.host,
		config.port,
		peers.peer_inbound_count(),    // Anzahl der eingehenden Verbindungen
		peers.peer_outbound_count(),   // Anzahl der ausgehenden Verbindungen
		peers.peer_count(),            // Gesamtanzahl der verbundenen Peers
		peers.most_work_peers().len(), // Anzahl der Peers mit der höchsten Arbeit
		total_count,                   // Gesamtanzahl der bekannten Peers
		healthy_count,                 // Anzahl der gesunden Peers
		banned_count,                  // Anzahl der gesperrten Peers
		defuncts.len(),                // Anzahl der defekten Peers
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

	// --- CONTINUE: Peer discovery and requesting peer lists, even if enough outbound peers ---
	let mut connected_peers: Vec<PeerAddr> = vec![];
	for p in peers.connected_peers() {
		trace!(
			"monitor_peers: {}:{} ask {} for more peers",
			config.host,
			config.port,
			p.info.addr,
		);
		let _ = p.send_peer_request(p2p::Capabilities::PEER_LIST);
		connected_peers.push(p.info.addr)
	}

	// Attempt to connect to preferred peers if there is some
	if let Some(preferred_peers) = preferred_peers_list {
		for p in preferred_peers {
			if !connected_peers.contains(&p) {
				let _ = tx.send(p);
			}
		}
	}

	// Retry defunct peers occasionally
	if !defuncts.is_empty() {
		defuncts.shuffle(&mut rng());
		let _ = peers.update_state(defuncts[0].addr, p2p::State::Healthy);
		let _ = peers.update_capabilities(defuncts[0].addr, p2p::Capabilities::UNKNOWN);
	}

	// Find new healthy peers from db and queue them up for connection attempts
	let max_peer_attempts = 128;
	let new_peers = peers.find_peers(
		p2p::State::Healthy,
		p2p::Capabilities::UNKNOWN,
		max_peer_attempts as usize,
	);

	for p in new_peers {
		if let Ok(false) = peers.is_known(p.addr) {
			let _ = tx.send(p.addr);
		}
	}
}

// Check if we have any pre-existing peer in db. If so, start with those,
// otherwise use the seeds provided.
fn connect_to_seeds_and_preferred_peers(
	peers: Arc<p2p::Peers>,
	tx: mpsc::Sender<PeerAddr>,
	seed_list: Vec<PeerAddr>,
	peers_preferred_list: Option<Vec<PeerAddr>>,
) {
	// check if we have some peers in db
	// look for peers that are able to give us other peers (via PEER_LIST capability)
	let healthy_peers = peers.find_peers(p2p::State::Healthy, p2p::Capabilities::PEER_LIST, 100);

	let other_peers = peers.find_peers(p2p::State::Defunct, p2p::Capabilities::UNKNOWN, 20);

	let mut peers = healthy_peers;

	peers.extend(other_peers);

	// if so, get their addresses, otherwise use our seeds
	let mut peer_addrs = if peers.len() > 3 {
		peers.iter().map(|p| p.addr).collect::<Vec<_>>()
	} else {
		seed_list
	};

	// If we have preferred peers add them to the connection
	match peers_preferred_list {
		Some(mut peers_preferred) => peer_addrs.append(&mut peers_preferred),
		None => trace!("No preferred peers"),
	};

	if peer_addrs.len() == 0 {
		warn!("No seeds were retrieved.");
	}

	// connect to this first set of addresses
	for addr in peer_addrs {
		tx.send(addr).unwrap();
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
	let max_outbound_attempts = 128;
	for addr in addrs.into_iter().take(max_outbound_attempts) {
		// ignore the duplicate connecting to same peer within 30 seconds
		let now = Utc::now();
		if let Some(last_connect_time) = connecting_history.get(&addr) {
			if *last_connect_time + Duration::seconds(connect_min_interval) > now {
				debug!(
					"peer_connect: ignore a duplicate request to {}. previous connecting time: {}",
					addr,
					last_connect_time.format("%H:%M:%S%.3f").to_string(),
				);
				continue;
			} else {
				if let Some(history) = connecting_history.get_mut(&addr) {
					*history = now;
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
					if p.send_peer_request(capab).is_ok() {
						let _ = peers_c.update_state(addr, p2p::State::Healthy);
					}
				}
				Err(_) => {
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
