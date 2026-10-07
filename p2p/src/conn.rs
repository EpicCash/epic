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

//! Provides a connection wrapper that handles the lower level tasks in sending
//! or receiving data from the TCP socket, as well as dealing with timeouts.
//!
//! Because of a few idiosyncracies in the Rust `TcpStream`, this has to use
//! async I/O to be able to both read *and* write on the connection. Which
//! forces us to go through some additional gymnastic to loop over the async
//! stream and make sure we get the right number of bytes out.

use crate::core::ser;
use crate::core::ser::{FixedLength, ProtocolVersion, StreamingReader};
use crate::msg::{
	read_body, read_discard, read_header, write_message, Msg, MsgHeader, MsgHeaderWrapper,
};
use crate::types::Error;
use crate::util::{RateCounter, RwLock};
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use std::thread::{self, JoinHandle};

pub const SEND_CHANNEL_CAP: usize = 100 * 10;

const HEADER_IO_TIMEOUT: Duration = Duration::from_millis(2000);
const CHANNEL_TIMEOUT: Duration = Duration::from_millis(1000);
const BODY_IO_TIMEOUT: Duration = Duration::from_millis(60000);

/// A trait to be implemented in order to receive messages from the
/// connection. Allows providing an optional response.
pub trait MessageHandler: Send + 'static {
	fn consume<'a>(
		&self,
		msg: Message<'a>,
		stopped: Arc<AtomicBool>,
		tracker: Arc<Tracker>,
	) -> Result<Option<Msg>, Error>;
}

// Macro to simplify the boilerplate around I/O and Grin error handling
macro_rules! try_break {
	($inner:expr) => {
		match $inner {
			Ok(v) => Some(v),
			Err(Error::Connection(ref e)) if e.kind() == io::ErrorKind::TimedOut => None,
			Err(Error::Connection(ref e)) if e.kind() == io::ErrorKind::WouldBlock => {
				// to avoid the heavy polling which will consume CPU 100%
				thread::sleep(Duration::from_millis(10));
				None
			}
			Err(Error::Store(_))
			| Err(Error::Chain(_))
			| Err(Error::Internal)
			| Err(Error::NoDandelionRelay) => None,
			Err(ref e) => {
				debug!("try_break: exit the loop: {:?}", e);
				break;
			}
		}
	};
}

macro_rules! try_header {
	($res:expr, $conn: expr) => {{
		match $conn.set_read_timeout(Some(HEADER_IO_TIMEOUT)) {
			Ok(v) => v,
			Err(e) => {
				debug!("try_header error: {:?}", e);
			}
		}
		try_break!($res)
	}};
}

/// A message as received by the connection. Provides access to the message
/// header lazily consumes the message body, handling its deserialization.
pub struct Message<'a> {
	pub header: MsgHeader,
	remaining: u64,
	stream: &'a mut dyn Read,
	version: ProtocolVersion,
}

impl<'a> Message<'a> {
	fn from_header(
		header: MsgHeader,
		stream: &'a mut dyn Read,
		version: ProtocolVersion,
	) -> Message<'a> {
		Message {
			remaining: header.msg_len,
			header,
			stream,
			version,
		}
	}

	#[cfg(test)]
	pub(crate) fn from_header_for_test(
		header: MsgHeader,
		stream: &'a mut dyn Read,
		version: ProtocolVersion,
	) -> Message<'a> {
		Self::from_header(header, stream, version)
	}

	/// Read the message body from the underlying connection
	pub fn body<T: ser::Readable>(&mut self) -> Result<T, Error> {
		read_body(&self.header, self.stream, self.version)
	}

	/// Read one item without crossing the enclosing message frame.
	pub fn streaming_read<T: ser::Readable>(&mut self) -> Result<(T, u64), Error> {
		let mut reader = StreamingReader::with_limit(self.stream, self.version, self.remaining);
		let item = T::read(&mut reader)?;
		let bytes_read = reader.total_bytes_read();
		self.remaining = self
			.remaining
			.checked_sub(bytes_read)
			.ok_or(Error::MsgLen)?;
		Ok((item, bytes_read))
	}

	pub fn copy_attachment(&mut self, len: usize, writer: &mut dyn Write) -> Result<usize, Error> {
		if len == 0 {
			return Ok(0);
		}
		//TODO: (Biz) can we use a constant here to make the 8000 less suspicous-looking?
		let mut buf = [0u8; 8000];
		let read_len = len.min(buf.len());
		let read = self.stream.read(&mut buf[..read_len])?;
		if read == 0 {
			return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
		}
		writer.write_all(&buf[..read])?;
		Ok(read)
	}
}

pub struct StopHandle {
	/// Channel to close the connection
	stopped: Arc<AtomicBool>,
	// we need Option to take ownhership of the handle in stop()
	reader_thread: Option<JoinHandle<()>>,
	writer_thread: Option<JoinHandle<()>>,
}

impl StopHandle {
	/// Schedule this connection to safely close via the async close_channel.
	pub fn stop(&self) {
		self.stopped.store(true, Ordering::Relaxed);
	}

	pub fn wait(&mut self) {
		if let Some(reader_thread) = self.reader_thread.take() {
			self.join_thread(reader_thread);
		}
		if let Some(writer_thread) = self.writer_thread.take() {
			self.join_thread(writer_thread);
		}
	}

	fn join_thread(&self, peer_thread: JoinHandle<()>) {
		// wait only if other thread is calling us, eg shutdown
		if thread::current().id() != peer_thread.thread().id() {
			debug!("waiting for thread {:?} exit", peer_thread.thread().id());
			if let Err(e) = peer_thread.join() {
				error!("failed to stop peer thread: {:?}", e);
			}
		} else {
			debug!(
				"attempt to stop thread {:?} from itself",
				peer_thread.thread().id()
			);
		}
	}
}

#[derive(Clone)]
pub struct ConnHandle {
	/// Channel to allow sending data through the connection
	pub send_channel: mpsc::SyncSender<Msg>,
}

impl ConnHandle {
	/// Send msg via the synchronous, bounded channel (sync_sender).
	/// Two possible failure cases -
	/// * Disconnected: Propagate this up to the caller so the peer connection can be closed.
	/// * Full: Our internal msg buffer is full. This is not a problem with the peer connection
	/// and we do not want to close the connection. We drop the msg rather than blocking here.
	/// If the buffer is full because there is an underlying issue with the peer
	/// and potentially the peer connection. We assume this will be handled at the peer level.
	pub fn send(&self, msg: Msg) -> Result<(), Error> {
		self.try_send(msg, true)
	}

	pub(super) fn send_request(&self, msg: Msg) -> Result<(), Error> {
		self.try_send(msg, false)
	}

	fn try_send(&self, msg: Msg, drop_if_full: bool) -> Result<(), Error> {
		match self.send_channel.try_send(msg) {
			Ok(()) => Ok(()),
			Err(mpsc::TrySendError::Disconnected(_)) => {
				Err(Error::Send("try_send disconnected".to_owned()))
			}
			Err(mpsc::TrySendError::Full(_)) => {
				if drop_if_full {
					debug!("conn_handle: try_send but buffer is full, dropping msg");
					Ok(())
				} else {
					Err(Error::Send("try_send full".to_owned()))
				}
			}
		}
	}
}

pub struct Tracker {
	/// Bytes we've sent.
	pub sent_bytes: Arc<RwLock<RateCounter>>,
	/// Bytes we've received.
	pub received_bytes: Arc<RwLock<RateCounter>>,
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::core::pow::Difficulty;
	use crate::msg::{Ping, Type};

	struct OneByteReader(&'static [u8]);

	impl Read for OneByteReader {
		fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
			if self.0.is_empty() {
				return Ok(0);
			}
			buf[0] = self.0[0];
			self.0 = &self.0[1..];
			Ok(1)
		}
	}

	#[test]
	fn attachment_copy_returns_after_each_socket_read() {
		let mut input = OneByteReader(b"abc");
		let header = MsgHeader::new(Type::TxHashSetArchive, 0);
		let mut message = Message::from_header(header, &mut input, ProtocolVersion::local());
		let mut output = Vec::new();

		assert_eq!(message.copy_attachment(3, &mut output).unwrap(), 1);
		assert_eq!(output, b"a");
	}

	#[test]
	fn full_channel_reports_request_as_unsent() {
		let (send_channel, _receiver) = mpsc::sync_channel(0);
		let handle = ConnHandle { send_channel };
		let msg = Msg::new(
			Type::Ping,
			Ping {
				total_difficulty: Difficulty::zero(),
				height: 0,
				local_timestamp: 0,
			},
			ProtocolVersion::local(),
		)
		.unwrap();

		assert!(handle.send_request(msg).is_err());
	}
}

impl Tracker {
	pub fn new() -> Tracker {
		let received_bytes = Arc::new(RwLock::new(RateCounter::new()));
		let sent_bytes = Arc::new(RwLock::new(RateCounter::new()));
		Tracker {
			received_bytes,
			sent_bytes,
		}
	}

	pub fn inc_received(&self, size: u64) {
		self.received_bytes.write().inc(size);
	}

	pub fn inc_sent(&self, size: u64) {
		self.sent_bytes.write().inc(size);
	}

	pub fn inc_quiet_received(&self, size: u64) {
		self.received_bytes.write().inc_quiet(size);
	}

	pub fn inc_quiet_sent(&self, size: u64) {
		self.sent_bytes.write().inc_quiet(size);
	}
}

/// Start listening on the provided connection and wraps it. Does not hang
/// the current thread, instead just returns a future and the Connection
/// itself.
pub fn listen<H>(
	stream: TcpStream,
	version: ProtocolVersion,
	tracker: Arc<Tracker>,
	handler: H,
) -> io::Result<(ConnHandle, StopHandle)>
where
	H: MessageHandler,
{
	let (send_tx, send_rx) = mpsc::sync_channel(SEND_CHANNEL_CAP);

	let stopped = Arc::new(AtomicBool::new(false));

	let conn_handle = ConnHandle {
		send_channel: send_tx,
	};

	let (reader_thread, writer_thread) = poll(
		stream,
		conn_handle.clone(),
		version,
		handler,
		send_rx,
		stopped.clone(),
		tracker,
	)?;

	Ok((
		conn_handle,
		StopHandle {
			stopped,
			reader_thread: Some(reader_thread),
			writer_thread: Some(writer_thread),
		},
	))
}

fn poll<H>(
	conn: TcpStream,
	conn_handle: ConnHandle,
	version: ProtocolVersion,
	handler: H,
	send_rx: mpsc::Receiver<Msg>,
	stopped: Arc<AtomicBool>,
	tracker: Arc<Tracker>,
) -> io::Result<(JoinHandle<()>, JoinHandle<()>)>
where
	H: MessageHandler,
{
	// Split out tcp stream out into separate reader/writer halves.
	let mut reader = conn.try_clone().expect("clone conn for reader failed");
	let mut writer = conn.try_clone().expect("clone conn for writer failed");
	let reader_stopped = stopped.clone();

	let reader_tracker = tracker.clone();
	let writer_tracker = tracker.clone();

	let reader_thread = thread::Builder::new()
		.name("peer_read".to_string())
		.spawn(move || {
			if let Err(e) = reader.set_read_timeout(Some(BODY_IO_TIMEOUT)) {
				error!("Failed to set read timeout: {:?}", e);
				return; // Beende den Thread sauber
			}

			loop {
				// check the read end
				match try_header!(read_header(&mut reader, version), &mut reader) {
					Some(MsgHeaderWrapper::Known(header)) => {
						let msg = Message::from_header(header, &mut reader, version);

						trace!(
							"Received message header, type {:?}, len {}.",
							msg.header.msg_type,
							msg.header.msg_len
						);

						// Increase received bytes counter
						reader_tracker.inc_received(MsgHeader::LEN as u64 + msg.header.msg_len);

						let resp_msg = try_break!(handler.consume(
							msg,
							reader_stopped.clone(),
							reader_tracker.clone()
						));
						if let Some(Some(resp_msg)) = resp_msg {
							try_break!(conn_handle.send(resp_msg));
						}
					}
					Some(MsgHeaderWrapper::Unknown(msg_len, type_byte)) => {
						debug!(
							"Received unknown message header, type {:?}, len {}.",
							type_byte, msg_len
						);
						// Increase received bytes counter
						reader_tracker.inc_received(MsgHeader::LEN as u64 + msg_len);

						try_break!(read_discard(msg_len, &mut reader));
					}
					None => {}
				}

				// check the close channel
				if reader_stopped.load(Ordering::Relaxed) {
					break;
				}
			}

			debug!(
				"Shutting down reader connection with {}",
				reader
					.peer_addr()
					.map(|a| a.to_string())
					.unwrap_or("?".to_owned())
			);
			let _ = reader.shutdown(Shutdown::Both);
		})?;

	let writer_thread = thread::Builder::new()
		.name("peer_write".to_string())
		.spawn(move || {
			if let Err(e) = writer.set_write_timeout(Some(BODY_IO_TIMEOUT)) {
				error!("Failed to set write timeout: {:?}", e);
				return; // Beende den Thread sauber
			}

			let mut retry_send = Err(());
			let mut failcount = 0;

			loop {
				let maybe_data = retry_send.or_else(|_| send_rx.recv_timeout(CHANNEL_TIMEOUT));
				retry_send = Err(());
				if let Ok(data) = maybe_data {
					failcount = 0;
					let written =
						try_break!(write_message(&mut writer, &data, writer_tracker.clone()));
					if written.is_none() {
						retry_send = Ok(data);
					}
				}

				if failcount >= 100 {
					error!("Too many failures, closing connection");
					break;
				}

				failcount += 1;

				// check the close channel
				if stopped.load(Ordering::Relaxed) {
					break;
				}
			}

			debug!(
				"Shutting down writer connection with {}",
				writer
					.peer_addr()
					.map(|a| a.to_string())
					.unwrap_or("?".to_owned())
			);
		})?;
	Ok((reader_thread, writer_thread))
}
