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

/// Epic server commands processing
use std::process::exit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use std::fs;
use std::fs::File;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use reqwest;

use clap::ArgMatches;
use ctrlc;

use crate::config::GlobalConfig;
use crate::core::global;
use crate::p2p::{PeerAddr, Seeding};
use crate::servers;
use crate::tui::ui;
use crate::util::zip;

/// wrap below to allow UI to clean up on stop
pub fn start_server(config: servers::ServerConfig) {
	// Only tries to download a snapshot of mainnet on mainnet
	if epic_core::global::is_mainnet() {
		maybe_set_chain_to_snapshot(PathBuf::from(config.db_root.to_owned()));
	}

	start_server_tui(config);
	// Just kill process for now, otherwise the process
	// hangs around until sigint because the API server
	// currently has no shutdown facility
	warn!("Shutting down...");
	thread::sleep(Duration::from_millis(1000));
	warn!("Shutdown complete.");
	exit(0);
}

fn maybe_set_chain_to_snapshot(chain_data_path: PathBuf) {
	let root = chain_data_path
		.parent()
		.expect("Could not resolve root directory");
	let flag_file = root.join("2-15-rollback-flag");

	if flag_file.exists() {
		debug!("rollback: flag file exists: {}", flag_file.display());
		return;
	} else {
		println!("Performing chain data rollback");
		info!(
			"rollback: replacing chain data: {}",
			chain_data_path.display()
		);
	}

	let payload_hash = "f1443086c2389e3dfe8ba8f559c3d5eeec817c4d4d85795e6ecda1484f06a9c8";
	let payload_url = "https://epiccash.s3-sa-east-1.amazonaws.com/chain_data_synced_861141.zip";
	let payload_path = root.join("chain_data_synced_861141.zip");

	println!("Downloading canonical chain data to rollback to");
	info!("rollback: GET {} > {}", payload_url, payload_path.display());

	if !payload_path.exists()
		|| payload_hash != global::get_file_sha256(payload_path.to_str().unwrap())
	{
		println!("Please wait: this may take a while");
		debug!("rollback: downloading...");
		let mut response = reqwest::get(payload_url).expect("Failed to download payload");
		assert!(response.status().is_success());

		debug!("rollback: saving to file");
		let mut file = fs::File::create(payload_path.as_path())
			.expect("Failed to open file for saving payload");
		response.copy_to(&mut file).expect("Failed to save payload");

		debug!("rollback: checking hash");
		let actual_hash = global::get_file_sha256(payload_path.to_str().unwrap());
		if payload_hash != actual_hash {
			error!("rollback: aborting: unexpected hash {}", actual_hash);
			exit(1);
		}
	} else {
		debug!("rollback: payload already downloaded");
	}

	if chain_data_path.exists() {
		let timestamp = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_secs();
		let backup_path =
			chain_data_path.with_file_name(format!("chain_data_original_{}", timestamp));
		println!(
			"Making backup of existing chain data to:\n  {}",
			backup_path.display()
		);
		info!(
			"rollback: backing up original chain data to: {}",
			backup_path.display()
		);
		fs::rename(&chain_data_path, backup_path)
			.expect("Failed to rename original chain data folder");
	} else {
		debug!("rollback: no pre-existing chain data to backup");
	}

	println!("Extracting new chain data");
	info!("rollback: extracting new chain data");

	zip::decompress(
		File::open(payload_path).expect("Could not open payload"),
		&root,
		|_| true,
	)
	.expect("Failed to unzip payload");

	File::create(flag_file).expect("Failed to touch flag file");

	println!("Done");
	info!("rollback: success");
}

fn start_server_tui(config: servers::ServerConfig) {
	// Run the UI controller.. here for now for simplicity to access
	// everything it might need
	if config.run_tui.unwrap_or(false) {
		warn!("Starting EPIC in UI mode...");
		servers::Server::start(config, |serv: servers::Server| {
			let mut controller = ui::Controller::new().unwrap_or_else(|e| {
				panic!("Error loading UI controller: {}", e);
			});
			controller.run(serv);
		})
		.unwrap();
	} else {
		warn!("Starting EPIC w/o UI...");
		servers::Server::start(config, |serv: servers::Server| {
			let running = Arc::new(AtomicBool::new(true));
			let r = running.clone();
			ctrlc::set_handler(move || {
				r.store(false, Ordering::SeqCst);
			})
			.expect("Error setting handler for both SIGINT (Ctrl+C) and SIGTERM (kill)");
			while running.load(Ordering::SeqCst) {
				thread::sleep(Duration::from_secs(1));
			}
			warn!("Received SIGINT (Ctrl+C) or SIGTERM (kill).");
			serv.stop();
		})
		.unwrap();
	}
}

/// Handles the server part of the command line, mostly running, starting and
/// stopping the Epic blockchain server. Processes all the command line
/// arguments to build a proper configuration and runs Epic with that
/// configuration.
pub fn server_command(
	server_args: Option<&ArgMatches<'_>>,
	mut global_config: GlobalConfig,
) -> i32 {
	global::set_mining_mode(
		global_config
			.members
			.as_mut()
			.unwrap()
			.server
			.clone()
			.chain_type,
	);

	// just get defaults from the global config
	let mut server_config = global_config.members.as_ref().unwrap().server.clone();
	if let Some(a) = server_args {
		if let Some(port) = a.value_of("port") {
			server_config.p2p_config.port = port.parse().unwrap();
		}

		if let Some(api_port) = a.value_of("api_port") {
			let default_ip = "0.0.0.0";
			server_config.api_http_addr = format!("{}:{}", default_ip, api_port);
		}

		if let Some(wallet_url) = a.value_of("wallet_url") {
			server_config
				.stratum_mining_config
				.as_mut()
				.unwrap()
				.wallet_listener_url = wallet_url.to_string();
		}

		if let Some(seeds) = a.values_of("seed") {
			let seed_addrs = seeds
				.filter_map(|x| x.parse().ok())
				.map(|x| PeerAddr(x))
				.collect();
			server_config.p2p_config.seeding_type = Seeding::List;
			server_config.p2p_config.seeds = Some(seed_addrs);
		}
	}

	if let Some(a) = server_args {
		match a.subcommand() {
			("run", _) => {
				start_server(server_config);
			}
			("", _) => {
				println!("Subcommand required, use 'epic help server' for details");
			}
			(cmd, _) => {
				println!(":: {:?}", server_args);
				panic!(
					"Unknown server command '{}', use 'epic help server' for details",
					cmd
				);
			}
		}
	} else {
		start_server(server_config);
	}
	0
}
