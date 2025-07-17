// Copyright 2020 The EPIC Developers
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

use crate::config::GlobalConfig;
use crate::core::global;
use crate::p2p::{PeerAddr, Seeding};
use crate::servers;
use crate::tui::ui;
use clap::ArgMatches;
use ctrlc;
use epic_util::logger::LogEntry;
use std::process::exit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub fn start_server(
    config: servers::ServerConfig,
    logs_rx: Option<mpsc::Receiver<LogEntry>>,
    api_chan: &'static mut (
        tokio::sync::oneshot::Sender<()>,
        tokio::sync::oneshot::Receiver<()>,
    ),
) {
    if let Err(e) = start_server_tui(config, logs_rx, api_chan) {
        error!("Failed to start server: {}", e);
        warn!("Shutting down...");
        thread::sleep(Duration::from_millis(1000));
        warn!("Shutdown complete.");
        exit(1);
    }
    warn!("Shutting down...");
    thread::sleep(Duration::from_millis(1000));
    warn!("Shutdown complete.");
    exit(0);
}

fn start_server_tui(
    config: servers::ServerConfig,
    logs_rx: Option<mpsc::Receiver<LogEntry>>,
    api_chan: &'static mut (
        tokio::sync::oneshot::Sender<()>,
        tokio::sync::oneshot::Receiver<()>,
    ),
) -> Result<(), String> {
    if config.run_tui.unwrap_or(false) {
        info!("Starting EPIC in UI mode...");
        servers::Server::start(
            config,
            logs_rx,
            |serv: servers::Server, logs_rx: Option<mpsc::Receiver<LogEntry>>| {
                let mut controller = match logs_rx {
                    Some(rx) => match ui::Controller::new(rx) {
                        Ok(ctrl) => ctrl,
                        Err(e) => {
                            error!("Error loading UI controller: {}", e);
                            return;
                        }
                    },
                    None => {
                        error!("No logs_rx provided for UI controller");
                        return;
                    }
                };
                controller.run(serv);
            },
            None,
            api_chan,
        )
        .map_err(|e| format!("Failed to start server in UI mode: {:?}", e))?;
    } else {
        info!("Starting EPIC w/o UI...");
        let msg = "Failed to start server without UI";
        servers::Server::start(
            config,
            logs_rx,
            |serv: servers::Server, _: Option<mpsc::Receiver<LogEntry>>| {
                let running = Arc::new(AtomicBool::new(true));
                let r = running.clone();
                if let Err(e) = ctrlc::set_handler(move || {
                    r.store(false, Ordering::SeqCst);
                }) {
                    error!("Error setting handler for SIGINT/SIGTERM: {}", e);
                    return;
                }
                while running.load(Ordering::SeqCst) {
                    thread::sleep(Duration::from_secs(1));
                }
                warn!("Received SIGINT (Ctrl+C) or SIGTERM (kill).");
                serv.stop();
            },
            None,
            api_chan,
        )
        .map_err(|e| format!("{}: {:?}", msg, e))?;
    }
    Ok(())
}

pub fn server_command(
    server_args: Option<&ArgMatches>,
    mut global_config: GlobalConfig,
    logs_rx: Option<mpsc::Receiver<LogEntry>>,
    api_chan: &'static mut (
        tokio::sync::oneshot::Sender<()>,
        tokio::sync::oneshot::Receiver<()>,
    ),
) -> i32 {
    let members = match global_config.members.as_mut() {
        Some(m) => m,
        None => {
            error!("Missing 'members' in global config");
            return 1;
        }
    };
    global::set_mining_mode(members.server.clone().chain_type);

    let mut server_config = match global_config.members.as_ref() {
        Some(m) => m.server.clone(),
        None => {
            error!("Missing 'members' in global config");
            return 1;
        }
    };

    if let Some(a) = server_args {
        if let Some(port) = a.get_one::<String>("port") {
            match port.parse() {
                Ok(p) => server_config.p2p_config.port = p,
                Err(e) => {
                    error!("Invalid port: {}", e);
                    return 1;
                }
            }
        }

        if let Some(api_port) = a.get_one::<String>("api_port") {
            let default_ip = "0.0.0.0";
            server_config.api_http_addr = format!("{}:{}", default_ip, api_port);
        }

        if let Some(wallet_url) = a.get_one::<String>("wallet_url") {
            if let Some(stratum_cfg) = server_config.stratum_mining_config.as_mut() {
                stratum_cfg.wallet_listener_url = wallet_url.to_string();
            } else {
                error!("No stratum_mining_config found in server config");
            }
        }

        if let Some(seeds) = a.get_many::<String>("seed") {
            let seed_addrs: Vec<PeerAddr> = seeds
                .filter_map(|x| x.parse().ok())
                .map(PeerAddr)
                .collect();
            server_config.p2p_config.seeding_type = Seeding::List;
            server_config.p2p_config.seeds = Some(seed_addrs);
        }
    }

    if let Some(a) = server_args {
        match a.subcommand() {
            Some(("run", _)) => {
                start_server(server_config, logs_rx, api_chan);
            }
            Some(("", _)) => {
                println!("Subcommand required, use 'epic server --help' for details");
            }
            Some((cmd, _)) => {
                println!(":: {:?}", server_args);
                error!(
                    "Unknown server command '{}', use 'epic server --help' for details",
                    cmd
                );
            }
            None => {
                println!("Subcommand required, use 'epic server --help' for details");
            }
        }
    } else {
        start_server(server_config, logs_rx, api_chan);
    }
    0
}
