use chrono::Utc;
use std::net::TcpStream;

/// Checks if the network connection is stable by attempting to connect to a known host.
pub fn is_network_stable() -> bool {
	//TODO: use seed address instead
	match TcpStream::connect("8.8.8.8:53") {
		Ok(_) => true,   // Connection successful
		Err(_) => false, // Connection failed
	}
}

/// Checks if the system was in standby mode by measuring the time difference between two timestamps.
pub fn is_system_in_standby(last_check: i64) -> bool {
	let now = Utc::now().timestamp();
	let diff = now - last_check;

	// If the time difference is greater than expected, the system might have been in standby mode
	diff > 60 // Example: 60 seconds tolerance
}
