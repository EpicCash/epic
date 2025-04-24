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

//! Storage implementation for peer data.

use chrono::Utc;
use num::FromPrimitive;
use rand::rng;
use rand::seq::SliceRandom;

use crate::core::ser::{self, Readable, Reader, Writeable, Writer};
use crate::types::{Capabilities, PeerAddr, ReasonForBan};
use epic_store::{self, option_to_not_found, to_key, Error};

const DB_NAME: &'static str = "peer";
const STORE_SUBPATH: &'static str = "peers";

const PEER_PREFIX: u8 = 'P' as u8;

// Types of messages
enum_from_primitive! {
	#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
	pub enum State {
		Healthy = 0,
		Banned = 1,
		Defunct = 2,
	}
}

/// Data stored for any given peer we've encountered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerData {
	/// Network address of the peer.
	pub addr: PeerAddr,
	/// What capabilities the peer advertises. Unknown until a successful
	/// connection.
	pub capabilities: Capabilities,
	/// The peer user agent.
	pub user_agent: String,
	/// State the peer has been detected with.
	pub flags: State,
	/// The time the peer was last banned
	pub last_banned: i64,
	/// The reason for the ban
	pub ban_reason: ReasonForBan,
	/// Time when we last connected to this peer.
	pub last_connected: i64,
	/// Local utc from peer
	pub local_timestamp: i64,
}

impl Writeable for PeerData {
	fn write<W: Writer>(&self, writer: &mut W) -> Result<(), ser::Error> {
		self.addr.write(writer)?;
		ser_multiwrite!(
			writer,
			[write_u32, self.capabilities.bits()],
			[write_bytes, &self.user_agent],
			[write_u8, self.flags as u8],
			[write_i64, self.last_banned],
			[write_i32, self.ban_reason as i32],
			[write_i64, self.last_connected],
			[write_i64, self.local_timestamp]
		);
		Ok(())
	}
}

impl Readable for PeerData {
	fn read(reader: &mut dyn Reader) -> Result<PeerData, ser::Error> {
		let addr = PeerAddr::read(reader)?;
		let capab = reader.read_u32()?;
		let ua = reader.read_bytes_len_prefix()?;
		let (fl, lb, br) = ser_multiread!(reader, read_u8, read_i64, read_i32);

		let lc = reader.read_i64();
		// this only works because each PeerData is read in its own vector and this
		// is the last data element
		let last_connected = match lc {
			Err(_) => Utc::now().timestamp(),
			Ok(lc) => lc,
		};

		let lt = reader.read_i64();
		let local_timestamp = match lt {
			Err(_) => Utc::now().timestamp(),
			Ok(lt) => lt,
		};

		let user_agent = String::from_utf8(ua).map_err(|_| ser::Error::CorruptedData)?;
		let capabilities = Capabilities::from_bits_truncate(capab);
		let ban_reason = ReasonForBan::from_i32(br).ok_or(ser::Error::CorruptedData)?;

		match State::from_u8(fl) {
			Some(flags) => Ok(PeerData {
				addr,
				capabilities,
				user_agent,
				flags,
				last_banned: lb,
				ban_reason,
				last_connected,
				local_timestamp,
			}),
			None => Err(ser::Error::CorruptedData),
		}
	}
}

/// Storage facility for peer data.
pub struct PeerStore {
	db: epic_store::Store,
}

impl PeerStore {
	/// Instantiates a new peer store under the provided root path.
	pub fn new(db_root: &str) -> Result<PeerStore, Error> {
		let db = epic_store::Store::new(db_root, Some(DB_NAME), Some(STORE_SUBPATH), None)?;
		Ok(PeerStore { db })
	}

	pub fn save_peer(&self, p: &PeerData) -> Result<(), Error> {
		debug!("save_peer: {:?} marked {:?}", p.addr, p.flags);

		let batch = self.db.batch()?;
		batch.put_ser(&peer_key(p.addr)[..], p)?;
		batch.commit()
	}

	pub fn get_peer(&self, peer_addr: PeerAddr) -> Result<PeerData, Error> {
		option_to_not_found(self.db.get_ser(&peer_key(peer_addr)[..]), || {
			format!("Peer at address: {}", peer_addr)
		})
	}

	pub fn exists_peer(&self, peer_addr: PeerAddr) -> Result<bool, Error> {
		self.db.exists(&peer_key(peer_addr)[..])
	}

	/// TODO - allow below added to avoid github issue reports
	#[allow(dead_code)]
	pub fn delete_peer(&self, peer_addr: PeerAddr) -> Result<(), Error> {
		let batch = self.db.batch()?;
		batch.delete(&peer_key(peer_addr)[..])?;
		batch.commit()
	}

	pub fn find_peers(
		&self,
		state: State,
		cap: Capabilities,
		count: usize,
	) -> Result<Vec<PeerData>, Error> {
		let mut peers = self
			.db
			.iter::<PeerData>(&to_key(PEER_PREFIX, &mut "".to_string().into_bytes()))?
			.map(|(_, v)| v)
			.filter(|p| p.flags == state && p.capabilities.contains(cap))
			.collect::<Vec<_>>();
		peers[..].shuffle(&mut rng());
		Ok(peers.iter().take(count).cloned().collect())
	}

	/// List all known peers
	/// Used for /v1/peers/all api endpoint
	pub fn all_peers(&self) -> Result<Vec<PeerData>, Error> {
		let key = to_key(PEER_PREFIX, &mut "".to_string().into_bytes());
		Ok(self
			.db
			.iter::<PeerData>(&key)?
			.map(|(_, v)| v)
			.collect::<Vec<_>>())
	}

	/// Convenience method to load a peer data, update its status and save it
	/// back. If new state is Banned its last banned time will be updated too.
	pub fn update_state(&self, peer_addr: PeerAddr, new_state: State) -> Result<(), Error> {
		let batch = self.db.batch()?;

		let mut peer =
			option_to_not_found(batch.get_ser::<PeerData>(&peer_key(peer_addr)[..]), || {
				format!("Peer at address: {}", peer_addr)
			})?;
		peer.flags = new_state;
		if new_state == State::Banned {
			peer.last_banned = Utc::now().timestamp();
		}

		batch.put_ser(&peer_key(peer_addr)[..], &peer)?;
		batch.commit()
	}

	/// Convenience method to load a peer data, update its ban reason and save it
	/// back. If the peer is banned, its last banned time will also be updated.
	pub fn update_ban_reason(
		&self,
		peer_addr: PeerAddr,
		ban_reason: ReasonForBan,
	) -> Result<(), Error> {
		let batch = self.db.batch()?;

		let mut peer =
			option_to_not_found(batch.get_ser::<PeerData>(&peer_key(peer_addr)[..]), || {
				format!("Peer at address: {}", peer_addr)
			})?;

		// Aktualisiere den Bann-Grund und die Bann-Zeit
		peer.ban_reason = ban_reason;
		peer.last_banned = Utc::now().timestamp();
		peer.flags = State::Banned;

		batch.put_ser(&peer_key(peer_addr)[..], &peer)?;
		batch.commit()
	}

	/// Deletes peers from the storage that satisfy some condition `predicate`
	pub fn delete_peers<F>(&self, predicate: F) -> Result<(), Error>
	where
		F: Fn(&PeerData) -> bool,
	{
		let mut to_remove = vec![];

		for x in self.all_peers()? {
			if predicate(&x) {
				to_remove.push(x)
			}
		}

		// Delete peers in single batch
		if !to_remove.is_empty() {
			let batch = self.db.batch()?;

			for peer in to_remove {
				batch.delete(&peer_key(peer.addr)[..])?;
			}

			batch.commit()?;
		}

		Ok(())
	}
}

// Ignore the port unless ip is loopback address.
fn peer_key(peer_addr: PeerAddr) -> Vec<u8> {
	to_key(PEER_PREFIX, &mut peer_addr.as_key().into_bytes())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::types::{PeerAddr, ReasonForBan};

	#[test]
	fn test_update_ban_reason() {
		let peer_addr = "127.0.0.1:13414".parse().map(PeerAddr).unwrap();
		let ban_reason = ReasonForBan::BadBlock;

		// Erstelle einen PeerStore und füge einen Peer hinzu
		let peer_store = PeerStore::new("/tmp/peer_store_test").unwrap();
		let peer_data = PeerData {
			addr: peer_addr.clone(),
			capabilities: Capabilities::UNKNOWN,
			user_agent: "test".to_string(),
			flags: State::Healthy,
			last_banned: 0,
			ban_reason: ReasonForBan::None,
			last_connected: Utc::now().timestamp(),
			local_timestamp: Utc::now().timestamp(),
		};
		peer_store.save_peer(&peer_data).unwrap();

		// Aktualisiere den Bann-Grund
		peer_store
			.update_ban_reason(peer_addr.clone(), ban_reason)
			.unwrap();

		// Überprüfe, ob der Bann-Grund korrekt gespeichert wurde
		let updated_peer = peer_store.get_peer(peer_addr).unwrap();
		assert_eq!(updated_peer.ban_reason, ban_reason);
		assert_eq!(updated_peer.flags, State::Banned);
		assert!(updated_peer.last_banned > 0);
	}
}
