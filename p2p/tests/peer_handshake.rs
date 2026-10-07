// Copyright 2026 The Epic Cash Developers
// Copyright 2018 The Grin Developers
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

use epic_core as core;
use epic_p2p as p2p;

use epic_util as util;
use epic_util::StopState;

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::{thread, time};

use crate::core::core::hash::Hash;
use crate::core::pow::Difficulty;
use crate::p2p::types::PeerAddr;
use crate::p2p::Peer;

use chrono::prelude::Utc;

fn open_port() -> u16 {
	// use port 0 to allow the OS to assign an open port
	// TcpListener's Drop impl will unbind the port as soon as
	// listener goes out of scope
	let listener = TcpListener::bind("127.0.0.1:0").unwrap();
	listener.local_addr().unwrap().port()
}

// Starts a server and connects a client peer to it to check handshake,
// followed by a ping/pong exchange to make sure the connection is live.
#[test]
fn peer_handshake() {
	util::init_test_logger();

	let p2p_config = p2p::P2PConfig {
		host: "127.0.0.1".parse().unwrap(),
		port: open_port(),
		peers_allow: None,
		peers_deny: None,
		..p2p::P2PConfig::default()
	};
	let net_adapter = Arc::new(p2p::DummyAdapter {});
	let server = Arc::new(
		p2p::Server::new(
			".epic",
			p2p::Capabilities::UNKNOWN,
			p2p_config.clone(),
			net_adapter.clone(),
			Hash::from_vec(&vec![]),
			Arc::new(StopState::new()),
			None,
		)
		.unwrap(),
	);

	let p2p_inner = server.clone();
	let _ = thread::spawn(move || p2p_inner.listen());

	thread::sleep(time::Duration::from_secs(1));

	let addr = SocketAddr::new(p2p_config.host, p2p_config.port);
	let socket = TcpStream::connect_timeout(&addr, time::Duration::from_secs(10)).unwrap();

	let my_addr = PeerAddr("127.0.0.1:5000".parse().unwrap());
	let peer = Peer::connect(
		socket,
		p2p::Capabilities::UNKNOWN,
		Difficulty::min(),
		my_addr,
		&p2p::handshake::Handshake::new(Hash::from_vec(&vec![]), p2p_config.clone()),
		net_adapter,
	)
	.unwrap();

	assert!(peer.info.user_agent.ends_with(env!("CARGO_PKG_VERSION")));

	thread::sleep(time::Duration::from_secs(1));

	peer.send_ping(Difficulty::min(), 0, Utc::now().timestamp())
		.unwrap();
	thread::sleep(time::Duration::from_secs(1));

	let server_peer = server.peers.get_connected_peer(my_addr).unwrap();
	assert_eq!(
		server_peer.info.advertised_total_difficulty(),
		Difficulty::min()
	);
	assert_eq!(
		server_peer.info.validated_total_difficulty(),
		Difficulty::zero()
	);
	assert!(server.peers.peer_count() > 0);
}

#[test]
fn genesis_mismatch_does_not_ban_client() {
	let server_config = p2p::P2PConfig {
		host: "127.0.0.1".parse().unwrap(),
		port: open_port(),
		peers_allow: None,
		peers_deny: None,
		..p2p::P2PConfig::default()
	};
	let root = tempfile::tempdir().unwrap();
	let server = Arc::new(
		p2p::Server::new(
			root.path().to_str().unwrap(),
			p2p::Capabilities::UNKNOWN,
			server_config.clone(),
			Arc::new(p2p::DummyAdapter {}),
			Hash::from_vec(&[1]),
			Arc::new(StopState::new()),
			None,
		)
		.unwrap(),
	);
	let listener = server.clone();
	let listener_thread = thread::spawn(move || listener.listen());
	let addr = SocketAddr::new(server_config.host, server_config.port);
	let stream = (0..50)
		.find_map(|_| TcpStream::connect(addr).ok().or_else(|| {
			thread::sleep(time::Duration::from_millis(20));
			None
		}))
		.expect("connect mismatched client");
	let client_addr = PeerAddr(stream.local_addr().unwrap());
	let client_config = p2p::P2PConfig {
		host: "127.0.0.1".parse().unwrap(),
		port: open_port(),
		..p2p::P2PConfig::default()
	};
	let result = Peer::connect(
		stream,
		p2p::Capabilities::UNKNOWN,
		Difficulty::min(),
		PeerAddr(SocketAddr::new(client_config.host, client_config.port)),
		&p2p::handshake::Handshake::new(Hash::from_vec(&[2]), client_config),
		Arc::new(p2p::DummyAdapter {}),
	);
	assert!(result.is_err());
	thread::sleep(time::Duration::from_millis(200));
	assert!(!server.peers.is_banned(client_addr));
	server.stop();
	listener_thread.join().unwrap().unwrap();
}
