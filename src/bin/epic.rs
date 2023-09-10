// Copyright 2020 The Grin Developers
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

//! Main for building the binary of a Epic peer-to-peer node.

#[macro_use]
extern crate clap;

#[macro_use]
extern crate log;
use crate::config::config::SERVER_CONFIG_FILE_NAME;
use crate::core::core::foundation;
use crate::core::{consensus, global};
use crate::util::init_logger;
use clap::App;
use epic_api as api;
use epic_chain as chain;
use epic_config as config;
use epic_core as core;
use epic_p2p as p2p;
use epic_servers as servers;
use epic_util as util;
use epic_util::logger::LogEntry;
use futures::channel::oneshot;
use servers::foundation::create_foundation;
use std::env;
use std::path::Path;
use std::sync::mpsc;

mod cmd;
pub mod tui;

// include build information
pub mod built_info {
	include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

pub fn info_strings() -> (String, String) {
	(
		format!(
			"This is Epic version {}{}, built for {} by {}.",
			built_info::PKG_VERSION,
			built_info::GIT_VERSION.map_or_else(|| "".to_owned(), |v| format!(" (git {})", v)),
			built_info::TARGET,
			built_info::RUSTC_VERSION,
		)
		.to_string(),
		format!(
			"Built with profile \"{}\", features \"{}\".",
			built_info::PROFILE,
			built_info::FEATURES_STR,
		)
		.to_string(),
	)
}

fn log_build_info() {
	let (basic_info, detailed_info) = info_strings();
	info!("{}", basic_info);
	debug!("{}", detailed_info);
}

fn main() {
	let exit_code = real_main();
	std::process::exit(exit_code);
}

fn real_main() -> i32 {
	let yml = load_yaml!("epic.yml");
	let args = App::from_yaml(yml)
		.version(built_info::PKG_VERSION)
		.get_matches();
	let node_config;

	let chain_type = if args.is_present("floonet") {
		global::ChainTypes::Floonet
	} else if args.is_present("usernet") {
		global::ChainTypes::UserTesting
	} else {
		global::ChainTypes::Mainnet
	};

	if let ("taxes", Some(taxes_args)) = args.subcommand() {
		global::set_mining_mode(chain_type.clone());
		let generate: u64 = taxes_args
			.value_of("generate")
			.unwrap()
			.parse()
			.unwrap_or_else(|e| {
				panic!("The generate value must be a positive integer: {}", e);
			});

		let url = taxes_args.value_of("from_wallet").unwrap().clone();
		let mut wallet_url = String::new();
		if !url.contains("http") {
			wallet_url.push_str("http://");
		}
		wallet_url.push_str(url);

		let path_str = taxes_args
			.value_of("path")
			.map(|p| Some(p.to_owned()))
			.unwrap_or_else(|| {
				let p = env::current_dir().ok()?;
				let s = p.to_str()?;
				Some(s.to_owned())
			})
			.expect("No valid path was found to save the foundation.json");
		let path = Path::new(path_str.as_str());
		assert_eq!(
			path.exists(),
			true,
			"The path: {} does not exist!",
			path.display()
		);
		let height: u64 = if let Some(h) = taxes_args.value_of("height") {
			h.parse().unwrap_or_else(|e| {
				panic!("The height value must be a positive integer: {}", e);
			})
		} else {
			consensus::foundation_height()
		};
		let foundation_coinbases = create_foundation(&wallet_url, generate, height);
		let serialized = foundation::serialize_foundation(foundation_coinbases);
		println!(
			"Total size in bytes serialized: {:?}",
			serialized.as_bytes().len()
		);
		foundation::save_in_disk(serialized, &path);
		return 0;
	}

	// Temporary wallet warning message
	match args.subcommand() {
		("wallet", _) => {
			println!();
			println!("As of v1.1.0, the wallet has been split into a separate executable.");
			println!("Please visit https://epic.tech/downloads/ to download");
			println!();
			return 0;
		}
		_ => {}
	}

	// Deal with configuration file creation
	match args.subcommand() {
		("server", Some(server_args)) => {
			// If it's just a server config command, do it and exit
			if let ("config", Some(_)) = server_args.subcommand() {
				cmd::config_command_server(&chain_type, SERVER_CONFIG_FILE_NAME);
				return 0;
			}
		}
		_ => {}
	}

	// Load relevant config
	match args.subcommand() {
		// When the subscommand is 'server' take into account the 'config_file' flag
		("server", Some(server_args)) => {
			if let Some(_path) = server_args.value_of("config_file") {
				node_config = Some(config::GlobalConfig::new(_path).unwrap_or_else(|e| {
					panic!("Error loading server configuration: {}", e);
				}));
			} else {
				node_config = Some(
					config::initial_setup_server(&chain_type).unwrap_or_else(|e| {
						panic!("Error loading server configuration: {}", e);
					}),
				);
			}
		}
		// Otherwise load up the node config as usual
		_ => {
			node_config = Some(
				config::initial_setup_server(&chain_type).unwrap_or_else(|e| {
					panic!("Error loading server configuration: {}", e);
				}),
			);
		}
	}

	let mut config = node_config.clone().unwrap();
	let mut logging_config = config.members.as_mut().unwrap().logging.clone().unwrap();
	logging_config.tui_running = config.members.as_mut().unwrap().server.run_tui;

	let api_chan: &'static mut (oneshot::Sender<()>, oneshot::Receiver<()>) =
		Box::leak(Box::new(oneshot::channel::<()>()));

	let (logs_tx, logs_rx) = if logging_config.tui_running.unwrap() {
		let (logs_tx, logs_rx) = mpsc::sync_channel::<LogEntry>(200);
		(Some(logs_tx), Some(logs_rx))
	} else {
		(None, None)
	};
	init_logger(Some(logging_config), logs_tx);

	global::set_mining_mode(config.members.unwrap().server.clone().chain_type);

	if let Some(file_path) = &config.config_file_path {
		info!(
			"Using configuration file at {}",
			file_path.to_str().unwrap()
		);
	} else {
		info!("Node configuration file not found, using default");
	};

	log_build_info();

	// Execute subcommand
	match args.subcommand() {
		// server commands and options
		("server", Some(server_args)) => {
			cmd::server_command(Some(server_args), node_config.unwrap(), logs_rx, api_chan)
		}

		// client commands and options
		("client", Some(client_args)) => cmd::client_command(client_args, node_config.unwrap()),

		// clean command
		("clean", _) => {
			let db_root_path = node_config.unwrap().members.unwrap().server.db_root;
			println!("Cleaning chain data directory: {}", db_root_path);
			match std::fs::remove_dir_all(db_root_path) {
				Ok(_) => 0,
				Err(_) => 1,
			}
		}

		// If nothing is specified, try to just use the config file instead
		// this could possibly become the way to configure most things
		// with most command line options being phased out
		_ => cmd::server_command(None, node_config.unwrap(), logs_rx, api_chan),
	}
}
