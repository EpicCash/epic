// Copyright 2025 The Epic Developers
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

use crate::cmd::built_info;
use clap::{Arg, Command};

pub fn build_cli() -> Command {
	Command::new("epic")
        .version(built_info::PKG_VERSION)
        .about("Lightweight implementation of the MimbleWimble protocol.")
        .author("The Epic Team")
        .arg(
            Arg::new("floonet")
                .long("floonet")
                .help("Run epic against the Floonet (as opposed to mainnet)")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("usernet")
                .long("usernet")
                .help("Run epic as a local-only network. Doesn't block peer connections but will not connect to any peer or seed")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("noprogpow")
                .long("noprogpow")
                .help("Run epic floonet or usernet without progpow blocks")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("onlyrandomx")
                .long("onlyrandomx")
                .help("Run epic floonet or usernet only with randomx blocks")
                .action(clap::ArgAction::SetTrue),
        )
        .subcommand(
            Command::new("taxes")
                .about("Generate kernels and outputs from the foundation wallet.")
                .arg(
                    Arg::new("from_wallet")
                        .short('w')
                        .long("from_wallet")
                        .help("The wallet listener ip from which the foundation outputs will be generated")
                        .required(true)
                        .value_name("FROM_WALLET"),
                )
                .arg(
                    Arg::new("generate")
                        .short('g')
                        .long("generate")
                        .help("The number (positive integer) of outputs that will be generated")
                        .required(true)
                        .value_name("GENERATE"),
                )
                .arg(
                    Arg::new("path")
                        .short('p')
                        .long("path")
                        .help("The path to the folder where the generated file will be saved. If no path is given, the current_directory/foundation is used.")
                        .value_name("PATH"),
                )
                .arg(
                    Arg::new("height")
                        .short('h')
                        .long("height")
                        .help("The height to start the generation of foundation coinbases. If no height is given, the FOUNDATION_HEIGHT is used.")
                        .value_name("HEIGHT"),
                ),
        )
        .subcommand(
            Command::new("wallet")
                .about("As of v1.1.0, the wallet has been split into a separate executable. See https://github.com/EpicCash/epic-wallet/releases")
                .override_usage("As of v1.1.0, the wallet has been split into a separate executable. See https://github.com/EpicCash/epic-wallet/releases to download"),
        )
        .subcommand(
            Command::new("server")
                .about("Control the Epic server")
                .arg(
                    Arg::new("config_file")
                        .short('c')
                        .long("config_file")
                        .help("Path to a epic-server.toml configuration file")
                        .value_name("CONFIG_FILE"),
                )
                .arg(
                    Arg::new("port")
                        .short('p')
                        .long("port")
                        .help("Port to start the P2P server on")
                        .value_parser(clap::value_parser!(u16)),
                )
                .arg(
                    Arg::new("api_port")
                        .short('a')
                        .long("api_port")
                        .help("Port on which to start the api server (e.g. transaction pool api)")
                        .value_parser(clap::value_parser!(u16)),
                )
                .arg(
                    Arg::new("seed")
                        .short('s')
                        .long("seed")
                        .help("Override seed node(s) to connect to")
                        .value_name("SEED"),
                )
                .arg(
                    Arg::new("wallet_url")
                        .short('w')
                        .long("wallet_url")
                        .help("The wallet listener to which mining rewards will be sent")
                        .value_name("WALLET_URL"),
                )
                .subcommand(
                    Command::new("config")
                        .about("Generate a configuration epic-server.toml file in the current directory"),
                )
                .subcommand(
                    Command::new("run")
                        .about("Run the Epic server in this console"),
                ),
        )
        .subcommand(
            Command::new("client")
                .about("Communicates with the Epic server")
                .subcommand(
                    Command::new("status")
                        .about("Current status of the Epic chain"),
                )
                .subcommand(
                    Command::new("listconnectedpeers")
                        .about("Print a list of currently connected peers"),
                )
                .subcommand(
                    Command::new("ban")
                        .about("Ban peer")
                        .arg(
                            Arg::new("peer")
                                .short('p')
                                .long("peer")
                                .help("Peer ip and port (e.g. 10.12.12.13:13414)")
                                .required(true)
                                .value_name("PEER"),
                        ),
                )
                .subcommand(
                    Command::new("unban")
                        .about("Unban peer")
                        .arg(
                            Arg::new("peer")
                                .short('p')
                                .long("peer")
                                .help("Peer ip and port (e.g. 10.12.12.13:13414)")
                                .required(true),
                        ),
                ),
        )
}
