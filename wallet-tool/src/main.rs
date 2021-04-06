use std::fs;
use std::io::{stdin, stdout, Write};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use dirs;

fn main() {
	println!("Epic wallet rollback tool\n");

	if let Some(s) = find_wallet_data_path() {
		run_wallet_check(PathBuf::from(s));
	}

	println!("Done");
}

fn run_wallet_check(wallet_data_path: PathBuf) {
	let timestamp = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap()
		.as_secs();
	let backup_path = wallet_data_path
		.parent()
		.expect("Failed to get parent folder")
		.join(format!("wallet_data_original_{}", timestamp));
	println!(
		"Making backup of existing wallet data:\n  {}",
		backup_path.display()
	);

	fs::rename(&wallet_data_path, &backup_path)
		.expect("Failed to rename original wallet data folder");

	fs::create_dir_all(&wallet_data_path).expect("Failed to recreate wallet data folder");
	fs::copy(
		backup_path.join("wallet.seed"),
		&wallet_data_path.join("wallet.seed"),
	)
	.expect("Failed to copy seed file back");

	println!("Restoring wallet");
	let status = Command::new("epic-wallet")
				.args(&["restore"])
				.status()
				.expect("Failed to restore wallet.\nPlease run:\n  epic-wallet recover\nand then:\n  epic-wallet restore\n");
	assert!(status.success());
}

fn find_wallet_data_path() -> Option<String> {
	println!("Trying to find wallet data path");

	let home_dir = dirs::home_dir().expect("Could not find home directory");
	let candidate = home_dir.join(".epic").join("main").join("wallet_data");
	if candidate.join("wallet.seed").is_file() {
		println!("Candidate found: {}", candidate.display());
	}

	print!("Please input the wallet data path to use (leave empty to skip):\n> ");
	stdout().flush().unwrap();

	let mut input = String::new();
	stdin().read_line(&mut input).expect("Invalid input");

	let trimmed_input = input.trim();
	if trimmed_input.is_empty() {
		println!("No wallet, skipping...");
		return None;
	}

	let path = Path::new(trimmed_input);
	if path.join("wallet.seed").is_file() {
		println!("Selected: {}", path.display());
	} else {
		println!("This is not a wallet directory; please try again");
		exit(1);
	}

	Some(trimmed_input.to_string())
}
