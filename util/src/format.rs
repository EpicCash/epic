/// Converts a size in bytes into a human-readable format.
pub fn human_readable_size(bytes: u64) -> String {
	const KB: u64 = 1024;
	const MB: u64 = KB * 1024;
	const GB: u64 = MB * 1024;

	if bytes >= GB {
		format!("{:.2} GB", bytes as f64 / GB as f64)
	} else if bytes >= MB {
		format!("{:.2} MB", bytes as f64 / MB as f64)
	} else if bytes >= KB {
		format!("{:.2} KB", bytes as f64 / KB as f64)
	} else {
		format!("{} bytes", bytes)
	}
}
