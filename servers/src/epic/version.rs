// Copyright 2026 The Epic Cash Developers
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

use crate::core::global;
use crate::core::global::Version;
use std::io::{self, Error, ErrorKind};
use std::str;
use hickory_resolver::proto::rr::RData;
use hickory_resolver::Resolver;
use tokio::runtime::Runtime;

//TODO: (Biz) find a much better way to do this

const MAINNET_DNS_VERSION: &str = "epicversion.epiccash.com.";

const FLOONET_DNS_VERSION: &str = "floonetversion.epiccash.com.";

pub fn get_dns_version() -> io::Result<Version> {
	let runtime = Runtime::new()?;
	let resolver = {
		let _guard = runtime.enter();
		Resolver::builder_tokio()
			.map_err(Error::other)?
			.build()
			.map_err(Error::other)?
	};

	let txt_lookup = if global::is_floonet() {
		FLOONET_DNS_VERSION
	} else {
		MAINNET_DNS_VERSION
	};
	info!("txt_lookup {:?}", txt_lookup);
	let response = runtime
		.block_on(resolver.txt_lookup(txt_lookup))
		.map_err(Error::other)?;

	let response_next = response
		.answers()
		.iter()
		.find_map(|record| match &record.data {
			RData::TXT(txt) => Some(txt),
			_ => None,
		})
		.ok_or(Error::new(
			ErrorKind::Other,
			"Invalid response when checking the node version!",
		))?;
	let version_next = response_next.txt_data.first().ok_or(Error::new(
		ErrorKind::Other,
		"Invalid response! Response doesn't include the node version!",
	))?;
	let version_string = str::from_utf8(version_next).map_err(|_e| {
		Error::new(
			ErrorKind::Other,
			"Invalid response! The version inside the response it's not a valid utf8 string!",
		)
	})?;
	let mut sanitezed = version_string.to_string();
	sanitezed.retain(|c| !r#"(),";:'"#.contains(c));
	let version_numbers: Vec<&str> = sanitezed.split(".").collect();
	if version_numbers.len() >= 2 {
		let version_major: u32 = if let Ok(number) = version_numbers[0].parse() {
			number
		} else {
			return Err(Error::new(
				ErrorKind::Other,
				"Invalid response! The response doesn't have a valid major version number, this number should be an integer!",
			));
		};
		let version_minor: u32 = if let Ok(number) = version_numbers[1].parse() {
			number
		} else {
			return Err(Error::new(
				ErrorKind::Other,
				"Invalid response! The response doesn't have a valid minor version number, this number should be an integer!",
			));
		};
		Ok(Version::new(version_major, version_minor))
	} else {
		return Err(Error::new(
			ErrorKind::Other,
			"Invalid response! The response doesn't have a valid version number (with a major and minor release)!",
		));
	}
}

/// Compare if the current version of this application is newer than the allowed version
pub fn is_version_valid(our_version: Version, allowed_version: Version) -> bool {
	our_version.version_major > allowed_version.version_major
		|| (our_version.version_major == allowed_version.version_major
			&& our_version.version_minor >= allowed_version.version_minor)
}
