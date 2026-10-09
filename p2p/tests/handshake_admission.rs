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
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
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
        Self::start_on(IpAddr::V4(Ipv4Addr::LOCALHOST), listener_buffer)
    }

    fn start_on(host: IpAddr, listener_buffer: u32) -> Self {
        let config = p2p::P2PConfig {
            host,
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

    fn connect(&self) -> TcpStream {
        self.connect_to(self.config.host)
    }

    fn connect_to(&self, host: IpAddr) -> TcpStream {
        let addr = SocketAddr::new(host, self.config.port);
        (0..50)
            .find_map(|_| {
                TcpStream::connect(addr).ok().or_else(|| {
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

#[derive(Clone, Copy)]
enum DistinctLocalAddress {
    LoopbackAlias(Ipv4Addr),
    Interface(Ipv4Addr),
}

impl DistinctLocalAddress {
    fn connect(self, port: u16) -> io::Result<TcpStream> {
        match self {
            Self::LoopbackAlias(source) => {
                let builder = TcpBuilder::new_v4()?;
                builder.bind(SocketAddr::new(IpAddr::V4(source), 0))?;
                builder.connect(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
            }
            Self::Interface(target) => {
                TcpStream::connect(SocketAddr::new(IpAddr::V4(target), port))
            }
        }
    }
}

fn distinct_local_address() -> Option<DistinctLocalAddress> {
    let alias = Ipv4Addr::new(127, 0, 0, 2);
    if TcpBuilder::new_v4()
        .and_then(|builder| {
            builder.bind(SocketAddr::new(IpAddr::V4(alias), 0))?;
            Ok(())
        })
        .is_ok()
    {
        return Some(DistinctLocalAddress::LoopbackAlias(alias));
    }

    // BSD/macOS commonly configures only 127.0.0.1 on loopback. A connected
    // UDP socket performs a route lookup without sending traffic and exposes
    // another local interface address that can reach a wildcard listener.
    let probe = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    probe.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match probe.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => {
            Some(DistinctLocalAddress::Interface(ip))
        }
        _ => None,
    }
}

fn connection_was_closed(stream: &mut TcpStream, wait: Duration) -> bool {
    stream
        .set_nonblocking(true)
        .expect("set closure probe nonblocking");
    let deadline = Instant::now() + wait;
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => return true,
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => {
                return matches!(
                    e.kind(),
                    io::ErrorKind::ConnectionAborted
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::BrokenPipe
                        | io::ErrorKind::NotConnected
                )
            }
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
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
    let distinct_address = match distinct_local_address() {
        Some(address) => address,
        None => {
            eprintln!("skipping cross-source admission test: no second local IPv4 address");
            return;
        }
    };
    let mut test_server = TestServer::start_on(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8);
    let silent = test_server.connect_to(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let silent_source = silent.local_addr().unwrap().ip();
    thread::sleep(Duration::from_millis(250));

    let config = test_server.config.clone();
    let port = test_server.config.port;
    let (result_tx, result_rx) = mpsc::channel();
    let honest = thread::spawn(move || {
        let stream = distinct_address.connect(port).unwrap();
        assert_ne!(stream.local_addr().unwrap().ip(), silent_source);
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
