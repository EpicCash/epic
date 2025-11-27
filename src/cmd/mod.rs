mod client;
mod config;
pub mod epic_args;
mod server;
pub use self::client::client_command;
pub use self::config::config_command_server;
pub use self::server::server_command;

pub mod built_info {
	include!(concat!(env!("OUT_DIR"), "/built.rs"));
}
