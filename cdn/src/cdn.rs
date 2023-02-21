use chrono::prelude::*;
use reqwest;
use std::env::current_dir;
use std::fs;
use std::fs::create_dir_all;
use std::io;
use std::io::Write;
use std::path::Path;
use zip;
//use std::sync::Arc;
use chrono::prelude::*;
use std::thread;
use std::time;
use tokio::runtime::Runtime;

pub async fn print_async(text: &str) {
	println!("{text}");
}

pub fn generate_name() -> String {
	let current_date = Local::now();
	format!("./download_{}.zip", current_date.format("%Y%m%d%H"))
}

pub fn run_download(path: String) {
	let mut rt = Runtime::new().unwrap();
	let hand = rt.spawn(async {
		let cdn_syncer = CDNSyncer {
			cdn: CDN {},
			download_path: path,
		};
		cdn_syncer.async_download().await;
	});

	rt.block_on(hand).unwrap();
}

pub struct CDNSyncer {
	pub cdn: CDN,
	pub download_path: String,
}

impl CDNSyncer {
	pub fn run(&self) {
		self.cdn.unzip();
		self.cdn
			.chdir("./chain_data", "/home/jualns/.epic/main/chain_data");
	}

	pub async fn async_download(&self) {
		print_async("++ run started").await;
		self.cdn.download(&self.download_path).await;
		print_async("++ download finished").await;
	}
}

pub struct CDN;

impl CDN {
	pub fn unzip(&self) {
		println!("++ UNZIP STARTED!");
		let fname = std::path::Path::new("./download.zip");
		let file = fs::File::open(&fname).unwrap();

		let mut archive = zip::ZipArchive::new(file).unwrap();

		for i in 0..archive.len() {
			let mut file = archive.by_index(i).unwrap();
			let outpath = match file.enclosed_name() {
				Some(path) => path.to_owned(),
				None => continue,
			};

			if (*file.name()).ends_with('/') {
				fs::create_dir_all(&outpath).unwrap();
			} else {
				if let Some(p) = outpath.parent() {
					if !p.exists() {
						fs::create_dir_all(p).unwrap();
					}
				}
				let mut outfile = fs::File::create(&outpath).unwrap();
				io::copy(&mut file, &mut outfile).unwrap();
			}

			// Get and Set permissions
			#[cfg(unix)]
			{
				use std::os::unix::fs::PermissionsExt;

				if let Some(mode) = file.unix_mode() {
					fs::set_permissions(&outpath, fs::Permissions::from_mode(mode)).unwrap();
				}
			}
		}
		println!("++ UNZIP FINISHED!");
	}

	pub async fn download(&self, download_path: &str) {
		print_async("++ DOWNLOAD STARTED!").await;
		let target = "https://epiccash.s3.sa-east-1.amazonaws.com/mainnet.zip";
		let response = reqwest::get(target);

		let resp = match response.await {
			Ok(r) => r,
			_ => panic!("error"),
		};

		let path = Path::new(download_path);

		if path.exists() {
			println!("Path exist");
			return;
		}

		let mut file = match fs::File::create(&path) {
			Err(why) => panic!("couldn't create {}", why),
			Ok(file) => file,
		};
		let content = resp.bytes();
		let content = match content.await {
			Ok(b) => b,
			Err(_) => panic!("error"),
		};
		print_async("++ DOWNLOAD FINISHED!").await;
		match file.write_all(&content) {
			Ok(_) => return,
			Err(_) => panic!("error"),
		};
	}

	pub fn chdir(&self, from: &str, to: &str) {
		println!("++ chdir started!");
		let path = current_dir().unwrap(); //Path::new("/home/~/.epic/main/cache_cdn/");

		if Path::new(to).exists() {
			let remove = fs::remove_dir_all(to);
			match remove {
				Ok(_) => (),
				Err(e) => panic!("Can't remove the original chain_data, error: {e}"),
			}
		}
		println!("++ Current Dir: {path:?}");
		match fs::rename(from, to) {
			Ok(_) => {
				return;
			}
			Err(e) => panic!("{}", e),
		};
	}
}
