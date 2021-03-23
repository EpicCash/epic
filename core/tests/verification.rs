//! Verification of commits

pub mod common;

use self::core::core::Output;
use epic_core as core;
use epic_keychain as keychain;

use serde::{Deserialize, Serialize};
use std::fs::File;

use std::io::BufReader;

#[derive(Debug, Deserialize, Serialize)]
struct Data {
	outputs: Vec<Output>,
}

#[test]
fn test_commit_verification() {
	println!("loading...");
	let file = File::open("./blocks.json").unwrap();
	let reader = BufReader::new(file);
	println!("deserializing...");
	let data: Data = serde_json::from_reader(reader).unwrap();

	println!("verifying...");
	for x in &data.outputs {
		match Output::verify_proof_single(&x.commit, &x.proof) {
			Ok(e) => println!("OK: {:?}, {:?}, {:?}", &x.commit, &x.proof, e),
			Err(e) => println!("ERR: {:?}, {:?}, {:?}", &x.commit, &x.proof, e),
		};
	}
}
