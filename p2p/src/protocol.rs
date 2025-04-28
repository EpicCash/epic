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
use crate::util::format::human_readable_size;

use crate::msg::{
	BanReason, FastHeaders, GetPeerAddrs, Headers, KernelDataResponse, Locator, LocatorFastSync,
	Msg, PeerAddrs, Ping, Pong, TxHashSetArchive, TxHashSetRequest, Type,
};
use crate::types::{Error, NetAdapter, PeerInfo};
use chrono::prelude::Utc;
use rand::{rng, Rng};
use std::cmp;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tempfile::tempfile;

pub struct Protocol {
	adapter: Arc<dyn NetAdapter>,
	peer_info: PeerInfo,
	state_sync_requested: Arc<AtomicBool>,
}

impl Protocol {
	pub fn new(
		adapter: Arc<dyn NetAdapter>,
		peer_info: PeerInfo,
		state_sync_requested: Arc<AtomicBool>,
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
				adapter.peer_difficulty(
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
				adapter.peer_difficulty(
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
				adapter.transaction_received(tx, false)?;
				Ok(None)
			}

			Type::StemTransaction => {
				debug!("Received stem tx: msg_len: {}", msg.header.msg_len);
				let tx: core::Transaction = msg.body()?;
				adapter.transaction_received(tx, true)?;
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
				let mut total_bytes_read = 0;

				// Read the count (u16) so we now how many headers to read.
				let (count, bytes_read): (u16, _) = msg.streaming_read()?;
				total_bytes_read += bytes_read;

				// Read chunks of headers off the stream and pass them off to the adapter.
				let mut headers = Headers {
					count: 0,
					headers: vec![],
				};
				let chunk_size = 128;
				for chunk in (0..count).collect::<Vec<_>>().chunks(chunk_size) {
					for _ in chunk {
						let (header, bytes_read) =
							msg.streaming_read::<core::UntrustedBlockHeader>()?;
						headers.headers.push(header.into());
						total_bytes_read += bytes_read;
					}
				}
				headers.headers.sort_by_key(|a| a.height);
				adapter.headers_received(&headers.headers, &self.peer_info)?;

				// Now check we read the correct total number of bytes off the stream.
				if total_bytes_read != msg.header.msg_len {
					return Err(Error::MsgLen);
				}

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

			Type::KernelDataRequest => {
				let kernel_data = self.adapter.kernel_data_read()?;
				let bytes = kernel_data.metadata()?.len();
				let kernel_data_response = KernelDataResponse { bytes };
				let mut response = Msg::new(
					Type::KernelDataResponse,
					&kernel_data_response,
					self.peer_info.version,
				)?;
				response.add_attachment(kernel_data);
				Ok(Some(response))
			}

			Type::KernelDataResponse => {
				let response: KernelDataResponse = msg.body()?;
				debug!("Kerneldata response bytes: {}", response.bytes);

				let mut writer = BufWriter::new(tempfile()?);

				let total_size = response.bytes as usize;
				let mut remaining_size = total_size;

				while remaining_size > 0 {
					let size = msg.copy_attachment(remaining_size, &mut writer)?;
					remaining_size = remaining_size.saturating_sub(size);

					// Increase received bytes quietly (without affecting the counters).
					// Otherwise we risk banning a peer as "abusive".
					tracker.inc_quiet_received(size as u64);
				}

				// Remember to seek back to start of the file as the caller is likely
				// to read this file directly without reopening it.
				writer.seek(SeekFrom::Start(0))?;

				let mut file = writer.into_inner().map_err(|_| Error::Internal)?;

				debug!(
					"Kerneldata response file size: {}",
					file.metadata().unwrap().len()
				);

				self.adapter.kernel_data_write(&mut file)?;

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
				if !self.state_sync_requested.load(Ordering::Relaxed) {
					debug!("Txhashset archive received but from the wrong peer",);
					return Err(Error::BadMessage);
				}

				let size = human_readable_size(sm_arch.bytes);
				info!(
					"Looking for Txhashset archive  {} at {}. size={}",
					sm_arch.hash, sm_arch.height, size,
				);

				// Update the sync state requested status
				self.state_sync_requested.store(true, Ordering::Relaxed);

				let download_start_time = Utc::now();
				self.adapter
					.txhashset_download_update(download_start_time, 0, sm_arch.bytes);

				let nonce: u32 = rng().random_range(1..1_000_000);
				let tmp = self.adapter.get_tmpfile_pathname(format!(
					"txhashset-{}-{}.zip",
					download_start_time.timestamp(),
					nonce
				));

				let mut save_txhashset_to_file = |file| -> Result<(), Error> {
					let mut tmp_zip = BufWriter::with_capacity(
						1_048_576, // 1 MB buffer
						OpenOptions::new().write(true).create_new(true).open(file)?,
					);
					let total_size = sm_arch.bytes as usize;
					let target_time = 600; // 10 minutes in seconds
					let average_upload_speed = 1_398_101; // 1.4 MB/s in bytes

					// Calculate optimal request_size
					let mut request_size = cmp::min(1_048_576, total_size); // Start with 1 MB
					if total_size > average_upload_speed * target_time {
						request_size = cmp::min(512_000, total_size); // Adjust to 512 KB for slower connections
					}

					let mut downloaded_size: usize = 0;
					let mut now = Instant::now();
					let download_start_time = Instant::now();

					while request_size > 0 {
						let size = msg.copy_attachment(request_size, &mut tmp_zip)?;
						downloaded_size += size;
						request_size = cmp::min(request_size, total_size - downloaded_size);

						// Calculate elapsed time and download speed
						let elapsed_time = download_start_time.elapsed().as_secs_f64();
						let download_speed = downloaded_size as f64 / elapsed_time; // Bytes per second
						let remaining_size = total_size - downloaded_size;
						let remaining_time = if download_speed > 0.0 {
							remaining_size as f64 / download_speed
						} else {
							0.0
						};

						// Update progress less frequently
						if downloaded_size % 1_000_000 == 0 || now.elapsed().as_secs() > 10 {
							self.adapter.txhashset_download_update(
								Utc::now(), // Use the current UTC time instead
								downloaded_size as u64,
								total_size as u64,
							);
							now = Instant::now();
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
					Ok(())
				};

				if let Err(e) = save_txhashset_to_file(tmp.clone()) {
					error!(
						"Txhashset archive save to file fail from peer {}. err={:?}",
						self.peer_info.addr, e
					);
					return Err(e);
				}

				trace!(
					"Txhashset archive save to file {:?} success from peer {}",
					tmp,
					self.peer_info.addr
				);

				let tmp_zip = File::open(tmp.clone())?;
				let res = self
					.adapter
					.txhashset_write(sm_arch.hash, tmp_zip, &self.peer_info)?;

				info!(
					"Txhashset archive for {} at {}, DONE. Data Ok: {} from peer {}",
					sm_arch.hash, sm_arch.height, !res, self.peer_info.addr
				);

				if let Err(e) = fs::remove_file(tmp.clone()) {
					warn!(
						"Txhashset archive fail to remove tmp file: {:?}. err: {} from peer {}",
						tmp, e, self.peer_info.addr
					);
				}

				Ok(None)
			}
			Type::Error | Type::Hand | Type::Shake => {
				debug!("Received an unexpected msg: {:?}", msg.header.msg_type);
				Ok(None)
			}
		}
	}
}
