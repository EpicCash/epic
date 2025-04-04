use crate::core::global;
use crate::core::global::Version;
use std::io::{self, Error, ErrorKind};
use std::str;
use trust_dns_resolver::config::*;
use trust_dns_resolver::Resolver;

const MAINNET_DNS_VERSION: &str = "epicversion.epic.tech.";

const FLOONET_DNS_VERSION: &str = "epicversion.51pool.online.";

pub fn get_dns_version() -> io::Result<Version> {
	let resolver = Resolver::new(ResolverConfig::default(), ResolverOpts::default())?;

	let txt_lookup = if global::is_floonet() {
		FLOONET_DNS_VERSION
	} else {
		MAINNET_DNS_VERSION
	};
	info!("txt_lookup {:?}", txt_lookup);
	let response = resolver.txt_lookup(txt_lookup)?;

	let response_next = response.iter().next().ok_or(Error::new(
		ErrorKind::Other,
		"Invalid response when checking the node version!",
	))?;
	let version_next = response_next.iter().next().ok_or(Error::new(
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
