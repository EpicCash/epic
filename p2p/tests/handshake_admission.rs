// Copyright 2026 The Epic Cash Developers
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

//! Handshake admission and deadline

use epic_core as core;
use epic_p2p as p2p;
use epic_util::StopState;
use net2::TcpBuilder;

use core::core::hash::Hash;
use core::pow::Difficulty;
use p2p::types::PeerAddr;
use p2p::Peer;

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

fn open_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

struct TestServer {
    server: Arc<p2p::Server>,
    config: p2p::P2PConfig,
    listener: Option<thread::JoinHandle<Result<(), p2p::Error>>>,
    _root: tempfile::TempDir,
}

impl TestServer {
    fn start(listener_buffer: u32) -> Self {
        let config = p2p::P2PConfig {
            host: "127.0.0.1".parse().unwrap(),
            port: open_port(),
            peers_allow: None,
            peers_deny: None,
            peer_listener_buffer_count: Some(listener_buffer),
            ..p2p::P2PConfig::default()
        };
        let root = tempfile::tempdir().unwrap();
        let server = Arc::new(
            p2p::Server::new(
                root.path().to_str().unwrap(),
                p2p::Capabilities::UNKNOWN,
                config.clone(),
                Arc::new(p2p::DummyAdapter {}),
                Hash::from_vec(&vec![]),
                Arc::new(StopState::new()),
                None,
            )
            .unwrap(),
        );
        let listener_server = server.clone();
        let listener = thread::spawn(move || listener_server.listen());

        Self {
            server,
            config,
            listener: Some(listener),
            _root: root,
        }
    }

    fn addr(&self) -> SocketAddr {
        SocketAddr::new(self.config.host, self.config.port)
    }

    fn connect(&self) -> TcpStream {
        (0..50)
            .find_map(|_| {
                TcpStream::connect(self.addr()).ok().or_else(|| {
                    thread::sleep(Duration::from_millis(20));
                    None
                })
            })
            .expect("connect to test server")
    }

    fn shutdown(&mut self) {
        self.server.stop();
        if let Some(listener) = self.listener.take() {
            listener.join().unwrap().unwrap();
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn connection_was_closed(stream: &mut TcpStream, wait: Duration) -> bool {
    stream.set_read_timeout(Some(wait)).unwrap();
    let mut byte = [0u8; 1];
    match stream.read(&mut byte) {
        Ok(0) => true,
        Err(e) => matches!(
            e.kind(),
            io::ErrorKind::ConnectionAborted
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::BrokenPipe
                | io::ErrorKind::NotConnected
        ),
        _ => false,
    }
}

fn connect_peer(test_server: &TestServer) -> Peer {
    let config = test_server.config.clone();
    Peer::connect(
        test_server.connect(),
        p2p::Capabilities::UNKNOWN,
        Difficulty::min(),
        PeerAddr(SocketAddr::new(config.host, open_port())),
        &p2p::handshake::Handshake::new(Hash::from_vec(&vec![]), config),
        Arc::new(p2p::DummyAdapter {}),
    )
    .expect("complete test handshake")
}

#[test]
fn silent_handshake_is_closed_by_upstream_timeout() {
    let mut test_server = TestServer::start(8);
    let mut silent = test_server.connect();

    let closed = connection_was_closed(&mut silent, Duration::from_secs(12));
    drop(silent);
    test_server.shutdown();

    assert!(closed, "silent pre-handshake socket remained open");
}

#[test]
fn timed_out_handshake_releases_capacity_without_banning_source() {
    let mut test_server = TestServer::start(1);
    let mut silent = test_server.connect();
    let client_addr = PeerAddr(silent.local_addr().unwrap());

    let closed = connection_was_closed(&mut silent, Duration::from_secs(12));
    drop(silent);
    thread::sleep(Duration::from_millis(250));
    let banned = test_server.server.peers.is_banned(client_addr);
    let peer = connect_peer(&test_server);

    drop(peer);
    test_server.shutdown();

    assert!(closed, "silent pre-handshake socket remained open");
    assert!(!banned, "timed-out source received a persistent ban");
}

#[test]
fn silent_handshake_does_not_block_honest_admission() {
    let mut test_server = TestServer::start(8);
    let silent = test_server.connect();
    thread::sleep(Duration::from_millis(250));

    let config = test_server.config.clone();
    let addr = test_server.addr();
    let (result_tx, result_rx) = mpsc::channel();
    let honest = thread::spawn(move || {
        let stream = TcpBuilder::new_v4()
            .unwrap()
            .bind(SocketAddr::new("127.0.0.2".parse().unwrap(), 0))
            .unwrap()
            .connect(addr)
            .unwrap();
        let result = Peer::connect(
            stream,
            p2p::Capabilities::UNKNOWN,
            Difficulty::min(),
            PeerAddr(SocketAddr::new(config.host, open_port())),
            &p2p::handshake::Handshake::new(Hash::from_vec(&vec![]), config),
            Arc::new(p2p::DummyAdapter {}),
        );
        result_tx.send(result.is_ok()).unwrap();
    });

    let admitted_while_silent = result_rx.recv_timeout(Duration::from_secs(2)).ok();
    drop(silent);
    let eventually_admitted = admitted_while_silent
        .or_else(|| result_rx.recv_timeout(Duration::from_secs(5)).ok())
        .unwrap_or(false);
    honest.join().unwrap();
    test_server.shutdown();

    assert!(eventually_admitted, "honest handshake never completed");
    assert_eq!(
        admitted_while_silent,
        Some(true),
        "silent handshake blocked the listener from admitting an honest peer"
    );
}

#[test]
fn trickled_handshake_expires_at_absolute_deadline() {
    let mut test_server = TestServer::start(8);
    let mut trickle = test_server.connect();
    thread::sleep(Duration::from_millis(250));

    // Keep the header's read_exact() making progress without ever completing its
    // 11 bytes. A per-read timeout restarts after each byte; an absolute handshake
    // deadline must still close this connection after ten seconds.
    for _ in 0..6 {
        let _ = trickle.write_all(&[0]);
        thread::sleep(Duration::from_secs(2));
    }

    let closed = connection_was_closed(&mut trickle, Duration::from_millis(500));
    drop(trickle);
    test_server.shutdown();

    assert!(closed, "trickled handshake outlived the absolute deadline");
}

#[test]
fn pending_handshake_capacity_is_bounded() {
    let mut test_server = TestServer::start(1);
    let first = test_server.connect();
    thread::sleep(Duration::from_millis(250));
    let mut excess = test_server.connect();

    let rejected = connection_was_closed(&mut excess, Duration::from_secs(1));
    drop(excess);
    drop(first);
    test_server.shutdown();

    assert!(
        rejected,
        "connection beyond pending-handshake capacity stayed open"
    );
}

#[test]
fn failed_handshake_releases_pending_capacity() {
    let mut test_server = TestServer::start(1);
    let failed = test_server.connect();
    thread::sleep(Duration::from_millis(250));
    drop(failed);
    thread::sleep(Duration::from_millis(250));

    let peer = connect_peer(&test_server);

    drop(peer);
    test_server.shutdown();
}

#[test]
fn malformed_handshake_releases_pending_capacity() {
    let mut test_server = TestServer::start(1);
    let mut malformed = test_server.connect();
    thread::sleep(Duration::from_millis(250));
    malformed.write_all(&[0; 11]).unwrap();

    let closed = connection_was_closed(&mut malformed, Duration::from_secs(2));
    drop(malformed);
    thread::sleep(Duration::from_millis(250));
    let peer = connect_peer(&test_server);

    drop(peer);
    test_server.shutdown();
    assert!(closed, "malformed handshake socket remained open");
}

#[test]
fn successful_handshake_releases_pending_capacity() {
    let mut test_server = TestServer::start(1);
    let first = connect_peer(&test_server);
    thread::sleep(Duration::from_millis(250));
    let second = connect_peer(&test_server);

    drop(second);
    drop(first);
    test_server.shutdown();
}

#[test]
fn one_ip_cannot_occupy_multiple_pending_slots() {
    let mut test_server = TestServer::start(2);
    let first = test_server.connect();
    thread::sleep(Duration::from_millis(250));
    let mut duplicate_ip = test_server.connect();

    let rejected = connection_was_closed(&mut duplicate_ip, Duration::from_secs(1));
    drop(duplicate_ip);
    drop(first);
    test_server.shutdown();

    assert!(rejected, "one source IP occupied multiple pending slots");
}

#[test]
fn listener_shutdown_is_bounded_with_pending_handshake() {
    let mut test_server = TestServer::start(1);
    let mut silent = test_server.connect();
    thread::sleep(Duration::from_millis(250));

    let shutdown_started = Instant::now();
    test_server.shutdown();
    let shutdown_elapsed = shutdown_started.elapsed();
    let worker_closed = connection_was_closed(&mut silent, Duration::from_secs(12));
    drop(silent);

    assert!(
        shutdown_elapsed < Duration::from_secs(2),
        "listener shutdown waited for a pending handshake"
    );
    assert!(
        worker_closed,
        "pending handshake outlived its deadline after listener shutdown"
    );
}
