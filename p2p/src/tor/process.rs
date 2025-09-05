// Copyright 2019 The Epic Developers
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

// BSD 3-Clause License
//
// Copyright (c) 2016, Dhole
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// * Redistributions of source code must retain the above copyright notice, this
//   list of conditions and the following disclaimer.
//
// * Redistributions in binary form must reproduce the above copyright notice,
//   this list of conditions and the following disclaimer in the documentation
//   and/or other materials provided with the distribution.
//
// * Neither the name of the copyright holder nor the names of its
//   contributors may be used to endorse or promote products derived from
//   this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
// OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! Tor process control
//! Derived from from from https://github.com/Dhole/rust-tor-controller.git

extern crate chrono;
extern crate regex;
extern crate timer;

use regex::Regex;
use std::fs::{self, File};
use std::io;
use std::io::Write;
use std::io::{BufRead, BufReader};
use std::path::{Path, MAIN_SEPARATOR};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc::channel;
use std::thread;
use sysinfo::{Pid, Process};

#[cfg(windows)]
const TOR_EXE_NAME: &'static str = "tor/tor.exe";
#[cfg(not(windows))]
const TOR_EXE_NAME: &'static str = "tor/tor";

#[derive(Debug)]
pub enum Error {
	Process(String),
	IO(io::Error),
	PID(String),
	Tor(String, Vec<String>),
	InvalidLogLine,
	InvalidBootstrapLine(String),
	Regex(regex::Error),
	ProcessNotStarted,
	Timeout,
}

pub struct ProcessManager {
	system: sysinfo::System,
}

impl ProcessManager {
	pub fn new() -> Self {
		let mut system = sysinfo::System::new_all();
		system.refresh_all();
		ProcessManager { system }
	}

	pub fn get_process(&self, pid: i32) -> Option<&Process> {
		self.system.process(Pid::from(pid as usize))
	}
}

pub struct TorProcess {
	tor_cmd: String,
	args: Vec<String>,
	torrc_path: Option<String>,
	completion_percent: u8,
	timeout: u32,
	working_dir: Option<String>,
	pub stdout: Option<BufReader<ChildStdout>>,
	pub process: Option<Child>,
	process_manager: ProcessManager,
}

impl TorProcess {
	pub fn new() -> Self {
		TorProcess {
			tor_cmd: TOR_EXE_NAME.to_string(),
			args: vec![],
			torrc_path: None,
			completion_percent: 100 as u8,
			timeout: 0 as u32,
			working_dir: None,
			stdout: None,
			process: None,
			process_manager: ProcessManager::new(),
		}
	}

	pub fn tor_cmd(&mut self, tor_cmd: &str) -> &mut Self {
		self.tor_cmd = tor_cmd.to_string();
		self
	}

	pub fn torrc_path(&mut self, torrc_path: &str) -> &mut Self {
		self.torrc_path = Some(torrc_path.to_string());
		self
	}

	pub fn arg(&mut self, arg: String) -> &mut Self {
		self.args.push(arg);
		self
	}

	pub fn args(&mut self, args: Vec<String>) -> &mut Self {
		for arg in args {
			self.arg(arg);
		}
		self
	}

	pub fn completion_percent(&mut self, completion_percent: u8) -> &mut Self {
		self.completion_percent = completion_percent;
		self
	}

	pub fn timeout(&mut self, timeout: u32) -> &mut Self {
		self.timeout = timeout;
		self
	}

	pub fn working_dir(&mut self, dir: &str) -> &mut Self {
		self.working_dir = Some(dir.to_string());
		self
	}

	// The tor process will have its stdout piped, so if the stdout lines are not consumed they
	// will keep accumulating over time, increasing the consumed memory.
	pub fn launch(&mut self) -> Result<&mut Self, Error> {


		let mut tor_exe_path = std::env::current_exe().expect("Failed to get current exe path");
		tor_exe_path.pop(); // remove the executable filename
		tor_exe_path.push(&self.tor_cmd); // append "tor/tor" or "tor/tor.exe"
		let mut tor = Command::new(tor_exe_path);

		if let Some(ref d) = self.working_dir {
			tor.current_dir(&d);
			let pid_file_name = format!("{}{}pid", d, MAIN_SEPARATOR);
			// kill off PID if its already running
			if Path::new(&pid_file_name).exists() {
				let pid = fs::read_to_string(&pid_file_name).map_err(|err| Error::IO(err))?;
				let pid = pid
					.parse::<i32>()
					.map_err(|err| Error::PID(format!("{:?}", err)))?;
				if let Some(process) = self.process_manager.get_process(pid) {
					let _ = process.kill();
				}
			}
		}
		if let Some(ref torrc_path) = self.torrc_path {
			tor.args(&vec!["-f", torrc_path]);
		}
		let mut tor_process = tor
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                let msg = format!(
                    "TOR executable (`{}`) not found. Please ensure TOR is installed and on the path: {:?}",
                    TOR_EXE_NAME, err
                );
                Error::Process(msg)
            })?;

		if let Some(ref d) = self.working_dir {
			// split out the process id, so if we don't exit cleanly
			// we can take it down on the next run
			let pid_file_name = format!("{}{}pid", d, MAIN_SEPARATOR);
			let mut file = File::create(pid_file_name).map_err(|err| Error::IO(err))?;
			file.write_all(format!("{}", tor_process.id()).as_bytes())
				.map_err(|err| Error::IO(err))?;
		}

		let stdout = BufReader::new(tor_process.stdout.take().unwrap());

		self.process = Some(tor_process);
		let completion_percent = self.completion_percent;

		let (stdout_tx, stdout_rx) = channel();
		let stdout_timeout_tx = stdout_tx.clone();

		let timer = timer::Timer::new();
		let _guard =
			timer.schedule_with_delay(chrono::Duration::seconds(self.timeout as i64), move || {
				stdout_timeout_tx.send(Err(Error::Timeout)).unwrap_or(());
			});
		let stdout_thread = thread::spawn(move || {
			stdout_tx
				.send(Self::parse_tor_stdout(stdout, completion_percent))
				.unwrap_or(());
		});
		match stdout_rx.recv().unwrap() {
			Ok(stdout) => {
				stdout_thread.join().unwrap();
				self.stdout = Some(stdout);
				Ok(self)
			}
			Err(err) => {
				self.kill().unwrap_or(());
				stdout_thread.join().unwrap();
				Err(err)
			}
		}
	}

	fn parse_tor_stdout(
		mut stdout: BufReader<ChildStdout>,
		completion_perc: u8,
	) -> Result<BufReader<ChildStdout>, Error> {
		let re_bootstrap = Regex::new(r"^\[notice\] Bootstrapped (?P<perc>[0-9]+)%(.*): ")
			.map_err(|err| Error::Regex(err))?;

		let timestamp_len = "May 16 02:50:08.792".len();
		let mut warnings = Vec::new();
		let mut raw_line = String::new();

		while stdout
			.read_line_lossy(&mut raw_line)
			.map_err(|err| Error::Process(format!("{}", err)))?
			> 0
		{
			{
				if raw_line.len() < timestamp_len + 1 {
					return Err(Error::InvalidLogLine);
				}
				let timestamp = &raw_line[..timestamp_len];
				let line = &raw_line[timestamp_len + 1..raw_line.len() - 1];
				info!("{} {}", timestamp, line);
				match line.split(' ').nth(0) {
					Some("[notice]") => {
						if let Some("Bootstrapped") = line.split(' ').nth(1) {
							let perc = re_bootstrap
								.captures(line)
								.and_then(|c| c.name("perc"))
								.and_then(|pc| pc.as_str().parse::<u8>().ok())
								.ok_or(Error::InvalidBootstrapLine(line.to_string()))?;

							if perc >= completion_perc {
								break;
							}
						}
					}
					Some("[warn]") => warnings.push(line.to_string()),
					Some("[err]") => return Err(Error::Tor(line.to_string(), warnings)),
					_ => (),
				}
			}
			raw_line.clear();
		}
		Ok(stdout)
	}

	pub fn kill(&mut self) -> Result<(), Error> {
		if let Some(ref mut process) = self.process {
			Ok(process
				.kill()
				.map_err(|err| Error::Process(format!("{}", err)))?)
		} else {
			Err(Error::ProcessNotStarted)
		}
	}
}

// This is copied from https://github.com/rust-lang/rust/blob/d3cba254e464303a6495942f3a831c2bbd7f1768/src/libstd/io/mod.rs#L2495,
// but converted into a "lossy" version
#[derive(Debug)]
pub struct LossyLines<B> {
	buf: B,
}

impl<B: BufReadLossy> Iterator for LossyLines<B> {
	type Item = io::Result<String>;

	fn next(&mut self) -> Option<io::Result<String>> {
		let mut buf = String::new();
		match self.buf.read_line_lossy(&mut buf) {
			Ok(0) => None,
			Ok(_n) => {
				if buf.ends_with('\n') {
					buf.pop();
					if buf.ends_with('\r') {
						buf.pop();
					}
				}
				Some(Ok(buf))
			}
			Err(e) => Some(Err(e)),
		}
	}
}

// A lossy way to read lines
pub trait BufReadLossy: BufRead {
	fn read_line_lossy(&mut self, buf: &mut String) -> io::Result<usize> {
		let mut buffer = Vec::new();
		let size = self.read_until(b'\n', &mut buffer)?;
		let s = String::from_utf8_lossy(&buffer);
		buf.push_str(&s);
		Ok(size)
	}

	fn lines_lossy(self) -> LossyLines<Self>
	where
		Self: Sized,
	{
		LossyLines { buf: self }
	}
}

// Implement `BufReadLossy` for all types that implement `BufRead`
impl<T: BufRead> BufReadLossy for T {}

impl Drop for TorProcess {
	// kill the child
	fn drop(&mut self) {
		
		self.kill().unwrap_or(());
		info!("Tor thread stopped");
	}
}
