// Copyright 2019 The Epic Develope;
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

//! Functions defining wallet 'addresses', i.e. ed2559 keys based on
//! a derivation path

use crate::util::from_hex;
use crate::util::secp::key::SecretKey;
use crate::Error;

use epic_keychain::{ChildNumber, Identifier, Keychain, SwitchCommitmentType};

use data_encoding::BASE32;
use ed25519_dalek::SigningKey as DalekSecretKey;
use ed25519_dalek::VerifyingKey as DalekPublicKey;

use sha3::{Digest, Sha3_256};

use blake2_rfc::blake2b::blake2b;

/// Derive a secret key given a derivation path and index
pub fn address_from_derivation_path<K>(
	keychain: &K,
	parent_key_id: &Identifier,
	index: u32,
) -> Result<SecretKey, Error>
where
	K: Keychain,
{
	let mut key_path = parent_key_id.to_path();
	// An output derivation for acct m/0
	// is m/0/0/0, m/0/0/1 (for instance), m/1 is m/1/0/0, m/1/0/1
	// Address generation path should be
	// for m/0: m/0/1/0, m/0/1/1
	// for m/1: m/1/1/0, m/1/1/1
	key_path.path[1] = ChildNumber::from(1);
	key_path.depth = key_path.depth + 1;
	key_path.path[key_path.depth as usize - 1] = ChildNumber::from(index);
	let key_id = Identifier::from_path(&key_path);
	let sec_key = keychain
		.derive_key(0, &key_id, &SwitchCommitmentType::None)
		.map_err(|e| Error::AddressDecoding(format!("derive_key error: {}", e)))?;
	let hashed = blake2b(32, &[], &sec_key.0[..]);
	Ok(
		SecretKey::from_slice(&keychain.secp(), &hashed.as_bytes()[..])
			.map_err(|e| Error::AddressDecoding(format!("SecretKey::from_slice error: {}", e)))?,
	)
}

/// Output ed25519 keypair given an rust_secp256k1 SecretKey
pub fn ed25519_keypair(sec_key: &SecretKey) -> Result<(DalekSecretKey, DalekPublicKey), Error> {
	let d_skey = DalekSecretKey::from_bytes(&sec_key.0);

	let d_pub_key: DalekPublicKey = (&d_skey).into();
	Ok((d_skey, d_pub_key))
}

/// Output ed25519 pubkey represented by string
pub fn ed25519_parse_pubkey(pub_key: &str) -> Result<DalekPublicKey, Error> {
	let bytes =
		from_hex(pub_key.to_owned()).map_err(|e| Error::AddressDecoding(format!("{}", e)))?;

	// Ensure the bytes vector has exactly 32 bytes
	let array: &[u8; 32] = bytes
		.as_slice()
		.try_into()
		.map_err(|_| Error::AddressDecoding("Public key must be 32 bytes".to_owned()))?;

	match DalekPublicKey::from_bytes(array) {
		Ok(k) => Ok(k),
		Err(_) => {
			return Err(Error::AddressDecoding("Not a valid public key".to_owned()))?;
		}
	}
}

/// Return the ed25519 public key represented in an onion address
pub fn pubkey_from_onion_v3(onion_address: &str) -> Result<DalekPublicKey, Error> {
	let mut input = onion_address.to_uppercase();
	if input.starts_with("HTTP://") || input.starts_with("HTTPS://") {
		input = input.replace("HTTP://", "");
		input = input.replace("HTTPS://", "");
	}
	if input.ends_with(".ONION") {
		input = input.replace(".ONION", "");
	}
	let orig_address_raw = input.clone();
	// for now, just check input is the right length and try and decode from base32
	if input.len() != 56 {
		return Err(Error::AddressDecoding(
			"Input address is wrong length".into(),
		))?;
	}

	let mut address = BASE32
		.decode(input.as_bytes())
		.map_err(|_| Error::AddressDecoding("Input address is not base 32".into()))?
		.to_vec();

	let _ = address.split_off(32);

	// Ensure the bytes vector has exactly 32 bytes
	let array: &[u8; 32] = address
		.as_slice()
		.try_into()
		.map_err(|_| Error::AddressDecoding("Public key must be 32 bytes".to_owned()))?;

	let key = match DalekPublicKey::from_bytes(array) {
		Ok(k) => k,
		Err(_) => {
			return Err(Error::AddressDecoding(
				"Provided onion V3 address is invalid (parsing key)".to_owned(),
			))?;
		}
	};
	let test_v3 = match onion_v3_from_pubkey(&key) {
		Ok(k) => k,
		Err(_) => {
			return Err(Error::AddressDecoding(
				"Provided onion V3 address is invalid (converting from pubkey)".to_owned(),
			))?;
		}
	};

	if test_v3.to_uppercase() != orig_address_raw.to_uppercase() {
		return Err(Error::AddressDecoding(
			"Provided onion V3 address is invalid (no match)".to_owned(),
		))?;
	}
	Ok(key)
}

/// Generate an onion address from an ed25519_dalek public key
pub fn onion_v3_from_pubkey(pub_key: &DalekPublicKey) -> Result<String, Error> {
	// calculate checksum
	let mut hasher = Sha3_256::new();
	hasher.update(b".onion checksum");
	hasher.update(pub_key.as_bytes());
	hasher.update([0x03u8]);
	let checksum = hasher.finalize();

	let mut address_bytes = pub_key.as_bytes().to_vec();
	address_bytes.push(checksum[0]);
	address_bytes.push(checksum[1]);
	address_bytes.push(0x03u8);

	let ret = BASE32.encode(&address_bytes);
	Ok(ret.to_lowercase())
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn onion_v3_conversion() {
		let onion_address = "2a6at2obto3uvkpkitqp4wxcg6u36qf534eucbskqciturczzc5suyid";

		let key = pubkey_from_onion_v3(onion_address).unwrap();
		println!("Key: {:?}", &key);

		let out_address = onion_v3_from_pubkey(&key).unwrap();
		println!("Address: {:?}", &out_address);

		assert_eq!(onion_address, out_address);
	}
}
