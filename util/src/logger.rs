// Copyright 2020 The Grin Developers
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

//! Logging wrapper to be used throughout all crates in the workspace
use crate::Mutex;
use std::ops::Deref;

use backtrace::Backtrace;
use std::{panic, thread};

use log::{Level, Record};
use log4rs;
use log4rs::append::console::ConsoleAppender;
use log4rs::append::file::FileAppender;
use log4rs::append::rolling_file::{
	policy::compound::roll::fixed_window::FixedWindowRoller,
	policy::compound::trigger::size::SizeTrigger, policy::compound::CompoundPolicy,
	RollingFileAppender,
};
use log4rs::append::Append;
use log4rs::config::{Appender, Config, Root};
use log4rs::encode::pattern::PatternEncoder;
use log4rs::encode::writer::simple::SimpleWriter;
use log4rs::encode::Encode;
use log4rs::filter::{threshold::ThresholdFilter, Filter, Response};
use std::error::Error;
use std::sync::mpsc;
use std::sync::mpsc::SyncSender;

lazy_static! {
	/// Flag to observe whether logging was explicitly initialised (don't output otherwise)
	static ref WAS_INIT: Mutex<bool> = Mutex::new(false);
	/// Flag to observe whether tui is running, and we therefore don't want to attempt to write
	/// panics to stdout
	static ref TUI_RUNNING: Mutex<bool> = Mutex::new(false);
	/// Static Logging configuration, should only be set once, before first logging call
	static ref LOGGING_CONFIG: Mutex<LoggingConfig> = Mutex::new(LoggingConfig::default());
}

const LOGGING_PATTERN: &str = "{d(%Y%m%d %H:%M:%S%.3f)} {h({l})} {M} - {m}{n}";
const STDOUT_PATTERN: &str = "{d(%Y-%m-%d %H:%M:%S%.3f)} {h({l})} {m}{n}";
/// 32 log files to rotate over by default
const DEFAULT_ROTATE_LOG_FILES: u32 = 32 as u32;

/// Log Entry
#[derive(Clone, Serialize, Debug)]
pub struct LogEntry {
	/// The log message
	pub log: String,
	/// The log levelO
	pub level: Level,
}

/// Logging config
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoggingConfig {
	/// whether to log to stdout
	pub log_to_stdout: bool,
	/// logging level for stdout
	#[serde(deserialize_with = "custom_level_serde::deserialize")]
	pub stdout_log_level: Level,
	/// whether to log to file
	pub log_to_file: bool,
	/// log file level
	#[serde(deserialize_with = "custom_level_serde::deserialize")]
	pub file_log_level: Level,
	/// Log file path
	pub log_file_path: String,
	/// Whether to append to log or replace
	pub log_file_append: bool,
	/// Size of the log in bytes to rotate over (optional)
	pub log_max_size: Option<u64>,
	/// Number of the log files to rotate over (optional)
	pub log_max_files: Option<u32>,
	/// Whether the tui is running (optional)
	pub tui_running: Option<bool>,
}

impl Default for LoggingConfig {
	fn default() -> LoggingConfig {
		LoggingConfig {
			log_to_stdout: true,
			stdout_log_level: Level::Warn,
			log_to_file: true,
			file_log_level: Level::Info,
			log_file_path: String::from("epic.log"),
			log_file_append: true,
			log_max_size: Some(1024 * 1024 * 16), // 16 megabytes default
			log_max_files: Some(DEFAULT_ROTATE_LOG_FILES),
			tui_running: None,
		}
	}
}

/// Module for custom deserialization of the [`Level`] type
mod custom_level_serde {
	use std::fmt;

	use log::Level;

	use serde::de::{
		DeserializeSeed, Deserializer, EnumAccess, Error, Unexpected, VariantAccess, Visitor,
	};

	/// Possible values for the log levels
	static LOG_LEVEL_NAMES: &[&str] = &["ERROR", "WARN", "INFO", "DEBUG", "TRACE", "WARNING"];

	/// Custom deserialization for [`Level`] type to accept values from v2 and v3
	pub fn deserialize<'de, D>(de: D) -> Result<Level, D::Error>
	where
		D: Deserializer<'de>,
	{
		struct LevelIdentifier;

		impl<'de> Visitor<'de> for LevelIdentifier {
			type Value = Level;

			fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
				f.pad("log level")
			}

			fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
			where
				E: Error,
			{
				self.visit_bytes(s.as_bytes())
			}

			fn visit_bytes<E>(self, val: &[u8]) -> Result<Self::Value, E>
			where
				E: Error,
			{
				match &val.to_ascii_lowercase()[..] {
					b"error" => Ok(Level::Error),
					b"warn" | b"warning" => Ok(Level::Warn),
					b"info" => Ok(Level::Info),
					b"debug" => Ok(Level::Debug),
					b"trace" => Ok(Level::Trace),
					_ => Err(Error::unknown_variant(
						&String::from_utf8_lossy(val),
						LOG_LEVEL_NAMES,
					)),
				}
			}

			fn visit_u64<E>(self, val: u64) -> Result<Self::Value, E>
			where
				E: Error,
			{
				match val {
					0 => Ok(Level::Error),
					1 => Ok(Level::Warn),
					2 => Ok(Level::Info),
					3 => Ok(Level::Debug),
					4 => Ok(Level::Trace),
					_ => Err(Error::invalid_value(Unexpected::Unsigned(val), &self)),
				}
			}
		}

		impl<'de> DeserializeSeed<'de> for LevelIdentifier {
			type Value = Level;

			fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
			where
				D: Deserializer<'de>,
			{
				deserializer.deserialize_identifier(LevelIdentifier)
			}
		}

		struct LevelEnum;

		impl<'de> Visitor<'de> for LevelEnum {
			type Value = Level;

			fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
				f.pad("log level")
			}

			fn visit_enum<A>(self, value: A) -> Result<Self::Value, A::Error>
			where
				A: EnumAccess<'de>,
			{
				let (level, variant) = value.variant_seed(LevelIdentifier)?;
				// Every variant is a unit variant.
				variant.unit_variant()?;
				Ok(level)
			}
		}

		de.deserialize_enum("Level", LOG_LEVEL_NAMES, LevelEnum)
	}
}

/// This filter is rejecting messages that doesn't start with "epic"
/// in order to save log space for only Epic-related records
#[derive(Debug)]
struct EpicFilter;

impl Filter for EpicFilter {
	fn filter(&self, record: &Record<'_>) -> Response {
		if let Some(module_path) = record.module_path() {
			if module_path.starts_with("epic") {
				return Response::Neutral;
			}
		}

		Response::Reject
	}
}

#[derive(Debug)]
struct ChannelAppender {
	output: Mutex<SyncSender<LogEntry>>,
	encoder: Box<dyn Encode>,
}

impl Append for ChannelAppender {
	fn append(&self, record: &Record) -> Result<(), Box<dyn Error + Sync + Send>> {
		let mut writer = SimpleWriter(Vec::new());
		self.encoder.encode(&mut writer, record)?;

		let log = String::from_utf8_lossy(writer.0.as_slice()).to_string();

		let _ = self.output.lock().try_send(LogEntry {
			log,
			level: record.level(),
		});

		Ok(())
	}

	fn flush(&self) {}
}

/// Initialize the logger with the given configuration
pub fn init_logger(config: Option<LoggingConfig>, logs_tx: Option<mpsc::SyncSender<LogEntry>>) {
	if let Some(c) = config {
		let tui_running = c.tui_running.unwrap_or(false);
		if tui_running {
			let mut tui_running_ref = TUI_RUNNING.lock();
			*tui_running_ref = true;
		}

		// Save current logging configuration
		let mut config_ref = LOGGING_CONFIG.lock();
		*config_ref = c.clone();

		let level_stdout = c.stdout_log_level.to_level_filter();
		let level_file = c.file_log_level.to_level_filter();

		// Determine minimum logging level for Root logger
		let level_minimum = if level_stdout > level_file {
			level_stdout
		} else {
			level_file
		};

		// Start logger
		let stdout = ConsoleAppender::builder()
			.encoder(Box::new(PatternEncoder::new(&STDOUT_PATTERN)))
			.build();

		let mut root = Root::builder();

		let mut appenders = vec![];

		if tui_running {
			let channel_appender = ChannelAppender {
				encoder: Box::new(PatternEncoder::new(&LOGGING_PATTERN)),
				output: Mutex::new(logs_tx.unwrap()),
			};

			appenders.push(
				Appender::builder()
					.filter(Box::new(ThresholdFilter::new(level_stdout)))
					.filter(Box::new(EpicFilter))
					.build("tui", Box::new(channel_appender)),
			);
			root = root.appender("tui");
		} else if c.log_to_stdout {
			appenders.push(
				Appender::builder()
					.filter(Box::new(ThresholdFilter::new(level_stdout)))
					.filter(Box::new(EpicFilter))
					.build("stdout", Box::new(stdout)),
			);
			root = root.appender("stdout");
		}

		if c.log_to_file {
			// If maximum log size is specified, use rolling file appender
			// or use basic one otherwise
			let filter = Box::new(ThresholdFilter::new(level_file));
			let file: Box<dyn Append> = {
				if let Some(size) = c.log_max_size {
					let count = c.log_max_files.unwrap_or_else(|| DEFAULT_ROTATE_LOG_FILES);
					let roller = FixedWindowRoller::builder()
						.build(&format!("{}.{{}}.gz", c.log_file_path), count)
						.unwrap();
					let trigger = SizeTrigger::new(size);

					let policy = CompoundPolicy::new(Box::new(trigger), Box::new(roller));

					Box::new(
						RollingFileAppender::builder()
							.append(c.log_file_append)
							.encoder(Box::new(PatternEncoder::new(&LOGGING_PATTERN)))
							.build(c.log_file_path, Box::new(policy))
							.expect("Failed to create logfile"),
					)
				} else {
					Box::new(
						FileAppender::builder()
							.append(c.log_file_append)
							.encoder(Box::new(PatternEncoder::new(&LOGGING_PATTERN)))
							.build(c.log_file_path)
							.expect("Failed to create logfile"),
					)
				}
			};

			appenders.push(
				Appender::builder()
					.filter(filter)
					.filter(Box::new(EpicFilter))
					.build("file", file),
			);
			root = root.appender("file");
		}

		let config = Config::builder()
			.appenders(appenders)
			.build(root.build(level_minimum))
			.unwrap();

		let _ = log4rs::init_config(config).unwrap();

		info!(
			"log4rs is initialized, file level: {:?}, stdout level: {:?}, min. level: {:?}",
			level_file, level_stdout, level_minimum
		);

		// Mark logger as initialized
		let mut was_init_ref = WAS_INIT.lock();
		*was_init_ref = true;
	}

	send_panic_to_log();
}

/// Initializes the logger for unit and integration tests
pub fn init_test_logger() {
	let mut was_init_ref = WAS_INIT.lock();
	if *was_init_ref.deref() {
		return;
	}
	let mut logger = LoggingConfig::default();
	logger.log_to_file = false;
	logger.stdout_log_level = Level::Debug;

	// Save current logging configuration
	let mut config_ref = LOGGING_CONFIG.lock();
	*config_ref = logger;

	let level_stdout = config_ref.stdout_log_level.to_level_filter();
	let level_minimum = level_stdout; // minimum logging level for Root logger

	// Start logger
	let stdout = ConsoleAppender::builder()
		.encoder(Box::new(PatternEncoder::default()))
		.build();

	let mut root = Root::builder();

	let mut appenders = vec![];

	{
		let filter = Box::new(ThresholdFilter::new(level_stdout));
		appenders.push(
			Appender::builder()
				.filter(filter)
				.filter(Box::new(EpicFilter))
				.build("stdout", Box::new(stdout)),
		);

		root = root.appender("stdout");
	}

	let config = Config::builder()
		.appenders(appenders)
		.build(root.build(level_minimum))
		.unwrap();

	let _ = log4rs::init_config(config).unwrap();

	info!(
		"log4rs is initialized, stdout level: {:?}, min. level: {:?}",
		level_stdout, level_minimum
	);

	*was_init_ref = true;
}

/// hook to send panics to logs as well as stderr
fn send_panic_to_log() {
	panic::set_hook(Box::new(|info| {
		let backtrace = Backtrace::new();

		let thread = thread::current();
		let thread = thread.name().unwrap_or("unnamed");

		let msg = match info.payload().downcast_ref::<&'static str>() {
			Some(s) => *s,
			None => match info.payload().downcast_ref::<String>() {
				Some(s) => &**s,
				None => "Box<Any>",
			},
		};

		match info.location() {
			Some(location) => {
				error!(
					"\nthread '{}' panicked at '{}': {}:{}{:?}\n\n",
					thread,
					msg,
					location.file(),
					location.line(),
					backtrace
				);
			}
			None => error!("thread '{}' panicked at '{}'{:?}", thread, msg, backtrace),
		}
		//also print to stderr
		let tui_running = TUI_RUNNING.lock().clone();
		if !tui_running {
			let config = LOGGING_CONFIG.lock();

			eprintln!(
				"Thread '{}' panicked with message:\n\"{}\"\nSee {} for further details.",
				thread, msg, config.log_file_path
			);
		}
	}));
}

#[cfg(test)]
mod logging_config {
	use super::{LoggingConfig, DEFAULT_ROTATE_LOG_FILES};

	use log::Level;
	use serde_test::{assert_de_tokens, assert_de_tokens_error, assert_tokens, Token};

	macro_rules! config_tokens {
		($stdout_log_level_name:expr, $file_log_level_name:expr) => {
			[
				Token::Struct {
					name: "LoggingConfig",
					len: 9,
				},
				Token::Str("log_to_stdout"),
				Token::Bool(true),
				Token::Str("stdout_log_level"),
				Token::UnitVariant {
					name: "Level",
					variant: $stdout_log_level_name,
				},
				Token::Str("log_to_file"),
				Token::Bool(true),
				Token::Str("file_log_level"),
				Token::UnitVariant {
					name: "Level",
					variant: $file_log_level_name,
				},
				Token::Str("log_file_path"),
				Token::String("epic.log"),
				Token::Str("log_file_append"),
				Token::Bool(true),
				Token::Str("log_max_size"),
				Token::Some,
				Token::U64(1024 * 1024 * 16),
				Token::Str("log_max_files"),
				Token::Some,
				Token::U32(DEFAULT_ROTATE_LOG_FILES),
				Token::Str("tui_running"),
				Token::None,
				Token::StructEnd,
			]
		};
	}

	fn level_from_str(s: &str) -> Level {
		match &s.to_ascii_lowercase()[..] {
			"error" => (Level::Error),
			"warn" | "warning" => (Level::Warn),
			"info" => (Level::Info),
			"debug" => (Level::Debug),
			"trace" => (Level::Trace),
			_ => panic!("Not known level from string {}", s),
		}
	}

	#[test]
	fn v2_loglevel_values() {
		const V2_VALUES: &[&str] = &["Error", "Warning", "Info", "Debug", "Trace"];

		for s in V2_VALUES {
			let tokens = config_tokens!(s, s);
			let config = LoggingConfig {
				stdout_log_level: level_from_str(s),
				file_log_level: level_from_str(s),
				..Default::default()
			};

			assert_de_tokens(&config, &tokens)
		}
	}

	#[test]
	fn v3_loglevel_values() {
		const V3_VALUES: &[&str] = &[
			"ERROR", "WARN", "INFO", "DEBUG", "TRACE", "error", "warn", "info", "debug", "trace",
		];

		for s in V3_VALUES {
			let tokens = config_tokens!(s, s);
			let config = LoggingConfig {
				stdout_log_level: level_from_str(s),
				file_log_level: level_from_str(s),
				..Default::default()
			};

			assert_de_tokens(&config, &tokens)
		}
	}
}
