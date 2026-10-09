// Copyright 2026 The Epic Cash Developers
// Copyright 2019 The Grin Developers
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

use crate::chain;
use crate::conn::{Message, MessageHandler, Tracker};
use crate::core::core::{self, hash::Hash, hash::Hashed, CompactBlock};
use crate::util::{format::human_readable_size, Mutex};

use crate::msg::{
	BanReason, FastHeaders, GetPeerAddrs, Headers, Locator, LocatorFastSync, Msg,
	OnionAddressResponse, PeerAddrs, Ping, Pong, TxHashSetArchive, TxHashSetRequest, Type,
};
use crate::types::{Error, NetAdapter, PeerInfo, MAX_BLOCK_HEADERS};
use chrono::prelude::Utc;
use fs2::FileExt;
use std::io::BufWriter;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;

//TODO: (Biz) can we clean this up at all? I don't like how unwieldy this looks
const TXHASHSET_DOWNLOAD_DEADLINE: Duration = Duration::from_secs(2 * 60 * 60);
const TXHASHSET_MIN_RATE_GRACE: Duration = Duration::from_secs(2 * 60);
const TXHASHSET_MIN_BYTES_PER_SEC: u64 = 16 * 1024;
const TXHASHSET_DISK_RESERVE: u64 = 256 * 1024 * 1024;
const TXHASHSET_READ_CHUNK: usize = 8_000;

pub(crate) struct TxHashSetRequestState {
	height: u64,
	hash: Hash,
	max_bytes: u64,
	requested_at: Instant,
}

impl TxHashSetRequestState {
	pub(crate) fn new(height: u64, hash: Hash, max_bytes: u64) -> Self {
		Self {
			height,
			hash,
			max_bytes,
			requested_at: Instant::now(),
		}
	}
}
// end TODO cleanup

pub struct Protocol {
	adapter: Arc<dyn NetAdapter>,
	peer_info: PeerInfo,
	state_sync_requested: Arc<Mutex<Option<TxHashSetRequestState>>>,
}

fn txhashset_response_matches(expected: &TxHashSetRequestState, height: u64, hash: Hash) -> bool {
	expected.height == height && expected.hash == hash
}

fn txhashset_archive_len(bytes: u64, max_bytes: u64) -> Result<usize, Error> {
	if bytes > max_bytes {
		return Err(Error::BadMessage);
	}
	usize::try_from(bytes).map_err(|_| Error::MsgLen)
}

fn txhashset_required_space(bytes: u64) -> Result<u64, Error> {
	bytes.checked_add(TXHASHSET_DISK_RESERVE).ok_or(Error::MsgLen)
}

fn txhashset_transfer_expired(
	requested_at: Instant,
	window_started_at: Instant,
	now: Instant,
	window_bytes: u64,
) -> bool {
	let window_elapsed = now.saturating_duration_since(window_started_at);
	now.saturating_duration_since(requested_at) >= TXHASHSET_DOWNLOAD_DEADLINE
		|| (window_elapsed >= TXHASHSET_MIN_RATE_GRACE
			&& window_bytes
				< window_elapsed
					.as_secs()
					.saturating_mul(TXHASHSET_MIN_BYTES_PER_SEC))
}

fn read_streamed_headers(msg: &mut Message<'_>) -> Result<Headers, Error> {
	let (count, mut total_bytes_read): (u16, _) = msg.streaming_read()?;
	if count > MAX_BLOCK_HEADERS as u16 {
		return Err(crate::core::ser::Error::TooLargeReadErr.into());
	}

	let mut headers = Headers {
		count,
		headers: Vec::with_capacity(count as usize),
	};
	for _ in 0..count {
		let (header, bytes_read) = msg.streaming_read::<core::UntrustedBlockHeader>()?;
		total_bytes_read = total_bytes_read
			.checked_add(bytes_read)
			.ok_or(Error::MsgLen)?;
		headers.headers.push(header.into());
	}
	if total_bytes_read != msg.header.msg_len {
		return Err(Error::MsgLen);
	}
	Ok(headers)
}

impl Protocol {
	pub fn new(
		adapter: Arc<dyn NetAdapter>,
		peer_info: PeerInfo,
		state_sync_requested: Arc<Mutex<Option<TxHashSetRequestState>>>,
	) -> Protocol {
		Protocol {
			adapter,
			peer_info,
			state_sync_requested,
		}
	}
}

impl MessageHandler for Protocol {
	fn consume(
		&self,
		mut msg: Message,
		stopped: Arc<AtomicBool>,
		tracker: Arc<Tracker>,
	) -> Result<Option<Msg>, Error> {
		let adapter = &self.adapter;

		// If we received a msg from a banned peer then log and drop it.
		// If we are getting a lot of these then maybe we are not cleaning
		// banned peers up correctly?
		if adapter.is_banned(self.peer_info.addr) {
			debug!(
				"Handler: consume: peer {:?} banned, received: {:?}, dropping.",
				self.peer_info.addr, msg.header.msg_type,
			);
			return Ok(None);
		}

		match msg.header.msg_type {
			Type::Ping => {
				let ping: Ping = msg.body()?;
				adapter.peer_advertised_difficulty(
					self.peer_info.addr,
					ping.total_difficulty,
					ping.height,
					ping.local_timestamp,
				);

				Ok(Some(Msg::new(
					Type::Pong,
					Pong {
						total_difficulty: adapter.total_difficulty()?,
						height: adapter.total_height()?,
						local_timestamp: Utc::now().timestamp(),
					},
					self.peer_info.version,
				)?))
			}

			Type::Pong => {
				let pong: Pong = msg.body()?;
				adapter.peer_advertised_difficulty(
					self.peer_info.addr,
					pong.total_difficulty,
					pong.height,
					pong.local_timestamp,
				);
				Ok(None)
			}

			Type::BanReason => {
				let ban_reason: BanReason = msg.body()?;
				error!("BanReason {:?}", ban_reason);
				Ok(None)
			}

			Type::TransactionKernel => {
				let h: Hash = msg.body()?;
				debug!("Received tx kernel: {}, msg_len: {}", h, msg.header.msg_len);
				adapter.tx_kernel_received(h, &self.peer_info)?;
				Ok(None)
			}

			Type::GetTransaction => {
				let h: Hash = msg.body()?;
				debug!("GetTransaction: {}, msg_len: {}", h, msg.header.msg_len,);
				let tx = adapter.get_transaction(h);
				if let Some(tx) = tx {
					Ok(Some(Msg::new(
						Type::Transaction,
						tx,
						self.peer_info.version,
					)?))
				} else {
					Ok(None)
				}
			}

			Type::Transaction => {
				debug!("Received tx: msg_len: {}", msg.header.msg_len);
				let tx: core::Transaction = msg.body()?;
				adapter.transaction_received(tx, false, &self.peer_info)?;
				Ok(None)
			}

			Type::StemTransaction => {
				debug!("Received stem tx: msg_len: {}", msg.header.msg_len);
				let tx: core::Transaction = msg.body()?;
				adapter.transaction_received(tx, true, &self.peer_info)?;
				Ok(None)
			}

			Type::GetBlock => {
				let h: Hash = msg.body()?;
				trace!("GetBlock: {}, msg_len: {}", h, msg.header.msg_len,);

				let bo = adapter.get_block(h);
				if let Some(b) = bo {
					return Ok(Some(Msg::new(Type::Block, b, self.peer_info.version)?));
				}
				Ok(None)
			}

			Type::Block => {
				debug!("Received block: msg_len: {}", msg.header.msg_len);
				let b: core::UntrustedBlock = msg.body()?;

				// We default to NONE opts here as we do not know know yet why this block was
				// received.
				// If we requested this block from a peer due to our node syncing then
				// the peer adapter will override opts to reflect this.
				adapter.block_received(b.into(), &self.peer_info, chain::Options::NONE)?;
				Ok(None)
			}

			Type::GetCompactBlock => {
				let h: Hash = msg.body()?;
				if let Some(b) = adapter.get_block(h) {
					let cb: CompactBlock = b.into();
					Ok(Some(Msg::new(
						Type::CompactBlock,
						cb,
						self.peer_info.version,
					)?))
				} else {
					Ok(None)
				}
			}

			Type::CompactBlock => {
				debug!("Received compact block: msg_len: {}", msg.header.msg_len);
				let b: core::UntrustedCompactBlock = msg.body()?;

				adapter.compact_block_received(b.into(), &self.peer_info)?;
				Ok(None)
			}

			Type::GetHeaders => {
				// load headers from the locator
				let loc: Locator = msg.body()?;
				let offset = 0 as u8;
				let headers = adapter.locate_headers(&loc.hashes, &offset)?;
				let len = headers.len();
				// serialize and send all the headers over
				Ok(Some(Msg::new(
					Type::Headers,
					Headers {
						count: len as u16,
						headers,
					},
					self.peer_info.version,
				)?))
			}

			Type::GetHeadersFastSync => {
				// load headers from the locator
				let loc: LocatorFastSync = msg.body()?;
				let headers = adapter.locate_headers(&loc.hashes, &loc.offset)?;
				let len = headers.len();
				// serialize and send all the headers over
				Ok(Some(Msg::new(
					Type::FastHeaders,
					FastHeaders {
						count: len as u16,
						headers,
					},
					self.peer_info.version,
				)?))
			}

			// "header first" block propagation - if we have not yet seen this block
			// we can go request it from some of our peers
			Type::Header => {
				let header: core::UntrustedBlockHeader = msg.body()?;
				adapter.header_received(header.into(), &self.peer_info)?;
				Ok(None)
			}
			Type::Headers => {
				let mut headers = read_streamed_headers(&mut msg)?;
				headers.headers.sort_by_key(|a| a.height);
				adapter.headers_received(&headers.headers, &self.peer_info)?;

				Ok(None)
			}
			Type::FastHeaders => {
				let mut loc: Headers = msg.body()?;

				loc.headers.sort_by_key(|a| a.height);
				adapter.headers_received(&loc.headers, &self.peer_info)?;
				Ok(None)
			}

			Type::GetPeerAddrs => {
				let get_peers: GetPeerAddrs = msg.body()?;
				let peers = adapter.find_peer_addrs(get_peers.capabilities);
				Ok(Some(Msg::new(
					Type::PeerAddrs,
					PeerAddrs { peers },
					self.peer_info.version,
				)?))
			}

			Type::PeerAddrs => {
				let peer_addrs: PeerAddrs = msg.body()?;
				adapter.peer_addrs_received(peer_addrs.peers);
				Ok(None)
			}

			Type::TxHashSetRequest => {
				let sm_req: TxHashSetRequest = msg.body()?;
				info!(
					"SetRequest Txhashset for {} at {}",
					sm_req.hash, sm_req.height
				);

				let txhashset_header = self.adapter.txhashset_archive_header()?;
				let txhashset_header_hash = txhashset_header.hash();
				if txhashset_header.height != sm_req.height
					|| txhashset_header_hash != sm_req.hash
				{
					debug!(
						"Requested txhashset target is not the archive this peer currently serves",
					);
					return Ok(None);
				}
				let txhashset = self.adapter.txhashset_read(txhashset_header_hash);

				// Note: Investigate why the last rangeproof is empty (None, None) when importing txhashset data.
				//Its always the last rangeproof that is empty.
				// This is maybe a bug in the code that creates the txhashset archive below.
				//# see commit 10debf500ad1a2ef87f9ded11a6b2fb2e49669d6
				if let Some(txhashset) = txhashset {
					let file_sz = txhashset.reader.metadata()?.len();
					let mut resp = Msg::new(
						Type::TxHashSetArchive,
						&TxHashSetArchive {
							height: txhashset_header.height as u64,
							hash: txhashset_header_hash,
							bytes: file_sz,
						},
						self.peer_info.version,
					)?;
					resp.add_attachment(txhashset.reader);
					Ok(Some(resp))
				} else {
					Ok(None)
				}
			}
			//TODO: partial download (resume)
			//prompt: Is it possible to resume a TxHashSet download, or is it impossible because it's a zip file?
			Type::TxHashSetArchive => {
				let sm_arch: TxHashSetArchive = msg.body()?;

				if !self.adapter.txhashset_receive_ready() {
					debug!("Txhashset archive received but SyncStatus not on TxHashsetDownload",);
					return Err(Error::BadMessage);
				}
				let request = self
					.state_sync_requested
					.lock()
					.take()
					.ok_or(Error::BadMessage)?;
				if !txhashset_response_matches(&request, sm_arch.height, sm_arch.hash) {
					debug!(
						"Txhashset archive did not match this connection's request",
					);
					return Err(Error::BadMessage);
				}
				let total_size = txhashset_archive_len(sm_arch.bytes, request.max_bytes)?;

				let size = human_readable_size(sm_arch.bytes);
				info!(
					"Looking for Txhashset archive  {} at {}. size={}",
					sm_arch.hash, sm_arch.height, size,
				);

				let download_start_time = Utc::now();
				self.adapter
					.txhashset_download_update(download_start_time, 0, sm_arch.bytes);

				let tmp_dir = self.adapter.get_tmp_dir();
				std::fs::create_dir_all(&tmp_dir)?;
				let required_space = txhashset_required_space(sm_arch.bytes)?;
				if fs2::available_space(&tmp_dir)? < required_space {
					return Err(Error::Connection(std::io::Error::new(
						std::io::ErrorKind::Other,
						"insufficient space for txhashset archive",
					)));
				}
				let mut tmp = NamedTempFile::new_in(&tmp_dir)?;
				tmp.as_file().allocate(sm_arch.bytes)?;

				{
					let mut tmp_zip = BufWriter::with_capacity(
						1_048_576, // 1 MB buffer
						tmp.as_file_mut(),
					);

					let mut downloaded_size: usize = 0;
					let mut progress_updated_at = Instant::now();
					let mut rate_window_started_at = request.requested_at;
					let mut rate_window_bytes = 0u64;

					while downloaded_size < total_size {
						let checked_at = Instant::now();
						if txhashset_transfer_expired(
							request.requested_at,
							rate_window_started_at,
							checked_at,
							rate_window_bytes,
						) {
							return Err(Error::Timeout);
						}
						if checked_at.saturating_duration_since(rate_window_started_at)
							>= TXHASHSET_MIN_RATE_GRACE
						{
							rate_window_started_at = checked_at;
							rate_window_bytes = 0;
						}
						let request_size = TXHASHSET_READ_CHUNK.min(total_size - downloaded_size);
						let size = msg.copy_attachment(request_size, &mut tmp_zip)?;
						downloaded_size = downloaded_size.checked_add(size).ok_or(Error::MsgLen)?;
						rate_window_bytes = rate_window_bytes
							.checked_add(size as u64)
							.ok_or(Error::MsgLen)?;
						if txhashset_transfer_expired(
							request.requested_at,
							rate_window_started_at,
							Instant::now(),
							rate_window_bytes,
						) {
							return Err(Error::Timeout);
						}

						if progress_updated_at.elapsed() > Duration::from_secs(10) {
							self.adapter.txhashset_download_update(
								download_start_time,
								downloaded_size as u64,
								total_size as u64,
							);
							progress_updated_at = Instant::now();
							let elapsed_time = request.requested_at.elapsed().as_secs_f64();
							let download_speed = downloaded_size as f64 / elapsed_time;
							let remaining_time =
								(total_size - downloaded_size) as f64 / download_speed;
							info!(
								"Downloading Txhashset archive: {}/{} from peer {}. Speed: {:.2} KB/s, Remaining time: {:.2} seconds",
								downloaded_size,
								total_size,
								self.peer_info.addr,
								download_speed / 1024.0, // Convert to KB/s
								remaining_time
							);
						}

						// Increase received bytes quietly
						tracker.inc_quiet_received(size as u64);

						// Check the close channel
						if stopped.load(Ordering::Relaxed) {
							debug!(
								"Stopping txhashset download early from peer {}",
								self.peer_info.addr
							);
							return Err(Error::ConnectionClose);
						}
					}

					info!(
						"Txhashset archive: {}/{} ... DOWNLOAD DONE from peer {}",
						downloaded_size, total_size, self.peer_info.addr
					);
					tmp_zip
						.into_inner()
						.map_err(|_| Error::Internal)?
						.sync_all()?;
				}

				let tmp_zip = tmp.reopen()?;
				let res = self
					.adapter
					.txhashset_write(sm_arch.hash, tmp_zip, &self.peer_info)?;

				info!(
					"Txhashset archive for {} at {}, DONE. Data Ok: {} from peer {}",
					sm_arch.hash, sm_arch.height, !res, self.peer_info.addr
				);

				Ok(None)
			}
			Type::Error | Type::Hand | Type::Shake => {
				debug!("Received an unexpected msg: {:?}", msg.header.msg_type);
				Ok(None)
			}
			Type::OnionAddressRequest => {
				if let Some(my_onion_addr) = &self.adapter.my_onion_addr() {
					let response = OnionAddressResponse {
						onion_addr: my_onion_addr.clone(),
					};
					Ok(Some(Msg::new(
						Type::OnionAddressResponse,
						response,
						self.peer_info.version,
					)?))
				} else {
					warn!("No onion address set for this node, cannot respond to onion address request.");
					Ok(None)
				}
			}
			Type::OnionAddressResponse => {
				let onion_msg: OnionAddressResponse = msg.body()?;
				if onion_msg.onion_addr.is_empty() {
					warn!("Received empty onion address in response from peer.");
				} else {
					info!("Received onion address from peer: {}", onion_msg.onion_addr);
					let mut live_info = self.peer_info.live_info.write();
					live_info.onion_addr = Some(onion_msg.onion_addr.clone());
				}
				Ok(None)
			}
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::core::pow::Difficulty;
	use crate::core::ser::{self, ProtocolVersion};
	use crate::msg::{MsgHeader, Type};
	use crate::serv::DummyAdapter;
	use crate::types::{Capabilities, Direction, PeerAddr, PeerLiveInfo};
	use crate::util::RwLock;
	use std::panic::{catch_unwind, AssertUnwindSafe};

	fn read_headers(body: &[u8]) -> Result<Headers, Error> {
		let mut input = body;
		let header = MsgHeader::new(Type::Headers, body.len() as u64);
		let mut msg =
			Message::from_header_for_test(header, &mut input, ProtocolVersion::local());
		read_streamed_headers(&mut msg)
	}

	fn legacy_md5_header_with_length(len: u64) -> Vec<u8> {
		let mut body = Vec::new();
		body.extend_from_slice(&1u16.to_be_bytes()); // one header
		body.extend_from_slice(&6u16.to_be_bytes()); // pre-fork header version
		body.extend_from_slice(&0u64.to_be_bytes()); // height
		body.extend_from_slice(&0i64.to_be_bytes()); // timestamp
		body.extend_from_slice(&[0; 32 * 6]); // five hashes and kernel offset
		body.extend_from_slice(&0u64.to_be_bytes()); // output MMR size
		body.extend_from_slice(&0u64.to_be_bytes()); // kernel MMR size
		body.extend_from_slice(&0u64.to_be_bytes()); // empty Difficulty map
		body.extend_from_slice(&0u32.to_be_bytes()); // secondary scaling
		body.extend_from_slice(&0u64.to_be_bytes()); // nonce
		// Legacy MD5 wire tag. It is decoded before current policy rejects it.
		body.push(1);
		body.push(16); // edge bits
		body.extend_from_slice(&len.to_be_bytes());
		body
	}

	#[test]
	fn streamed_headers_reject_count_above_protocol_limit() {
		let body = (MAX_BLOCK_HEADERS as u16 + 1).to_be_bytes();
		assert!(matches!(
			read_headers(&body),
			Err(Error::Serialization(ser::Error::TooLargeReadErr))
		));
	}

	#[test]
	fn streamed_headers_reject_trailing_frame_bytes() {
		assert!(matches!(read_headers(&[0, 0, 0]), Err(Error::MsgLen)));
	}

	#[test]
	fn streamed_headers_accept_exact_empty_batch() {
		let headers = read_headers(&[0, 0]).unwrap();
		assert_eq!(headers.count, 0);
		assert!(headers.headers.is_empty());
	}

	#[test]
	fn streamed_legacy_md5_length_is_rejected_before_allocation() {
		let body = legacy_md5_header_with_length(u64::MAX);
		let mut input = &body[..];
		let header = MsgHeader::new(Type::Headers, body.len() as u64);
		let msg =
			Message::from_header_for_test(header, &mut input, ProtocolVersion::local());
		let peer_info = PeerInfo {
			capabilities: Capabilities::UNKNOWN,
			user_agent: "stream-budget".into(),
			version: ProtocolVersion::local(),
			addr: PeerAddr("127.0.0.1:3414".parse().unwrap()),
			direction: Direction::Inbound,
			live_info: Arc::new(RwLock::new(PeerLiveInfo::new(Difficulty::zero()))),
		};
		let protocol = Protocol::new(
			Arc::new(DummyAdapter {}),
			peer_info,
			Arc::new(Mutex::new(None)),
		);
		let result = catch_unwind(AssertUnwindSafe(|| {
			protocol.consume(
				msg,
				Arc::new(AtomicBool::new(false)),
				Arc::new(Tracker::new()),
			)
		}));

		assert!(matches!(
			result,
			Ok(Err(Error::Serialization(ser::Error::TooLargeReadErr)))
		));
	}

	#[test]
	fn txhashset_response_requires_exact_requested_height_and_hash() {
		let hash = Hash::from_vec(&[1]);
		let request = TxHashSetRequestState::new(100, hash, 1024);
		assert!(txhashset_response_matches(&request, 100, hash));
		assert!(!txhashset_response_matches(&request, 101, hash));
		assert!(!txhashset_response_matches(
			&request,
			100,
			Hash::from_vec(&[2]),
		));
	}

	#[test]
	fn txhashset_archive_size_rejects_only_values_over_limit() {
		assert_eq!(txhashset_archive_len(999, 1000).unwrap(), 999);
		assert_eq!(txhashset_archive_len(1000, 1000).unwrap(), 1000);
		assert!(matches!(
			txhashset_archive_len(1001, 1000),
			Err(Error::BadMessage)
		));
	}

	#[test]
	fn txhashset_disk_budget_keeps_reserve_and_checks_overflow() {
		assert_eq!(
			txhashset_required_space(1024).unwrap(),
			1024 + TXHASHSET_DISK_RESERVE
		);
		assert!(matches!(
			txhashset_required_space(u64::MAX),
			Err(Error::MsgLen)
		));
	}

	#[test]
	fn txhashset_transfer_has_absolute_and_rate_deadlines() {
		let start = Instant::now();
		assert!(!txhashset_transfer_expired(
			start,
			start,
			start + TXHASHSET_MIN_RATE_GRACE,
			TXHASHSET_MIN_RATE_GRACE.as_secs() * TXHASHSET_MIN_BYTES_PER_SEC,
		));
		assert!(txhashset_transfer_expired(
			start,
			start,
			start + TXHASHSET_MIN_RATE_GRACE,
			TXHASHSET_MIN_RATE_GRACE.as_secs() * TXHASHSET_MIN_BYTES_PER_SEC - 1,
		));
		assert!(txhashset_transfer_expired(
			start,
			start + TXHASHSET_MIN_RATE_GRACE,
			start + 2 * TXHASHSET_MIN_RATE_GRACE,
			TXHASHSET_MIN_RATE_GRACE.as_secs() * TXHASHSET_MIN_BYTES_PER_SEC - 1,
		));
		assert!(txhashset_transfer_expired(
			start,
			start + TXHASHSET_DOWNLOAD_DEADLINE - Duration::from_secs(1),
			start + TXHASHSET_DOWNLOAD_DEADLINE,
			u64::MAX,
		));
	}

	#[test]
	fn txhashset_tempfile_is_removed_on_drop() {
		let dir = tempfile::tempdir().unwrap();
		let path = {
			let file = NamedTempFile::new_in(dir.path()).unwrap();
			let path = file.path().to_owned();
			assert!(path.exists());
			path
		};
		assert!(!path.exists());
	}
}
