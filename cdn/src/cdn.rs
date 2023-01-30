use reqwest;
use std::fs;
use std::fs::File;
use std::io;
use std::io::Write;
use std::path::Path;
use zip;

pub struct CDN;

impl CDN {
	pub fn unzip(&self) {
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
	}

	pub async fn download(&self) {
		let target = "https://epiccash.s3.sa-east-1.amazonaws.com/mainnet.zip";
		let response = reqwest::get(target);

		let resp = match response.await {
			Ok(r) => r,
			_ => panic!("error"),
		};

		let path = Path::new("./download.zip");

		let mut file = match File::create(&path) {
			Err(why) => panic!("couldn't create {}", why),
			Ok(file) => file,
		};
		let content = resp.bytes();
		let content = match content.await {
			Ok(b) => b,
			Err(_) => panic!("error"),
		};
		match file.write_all(&content) {
			Ok(_) => return,
			Err(_) => panic!("error"),
		};
	}

	pub fn chdir(&self) {
		match fs::rename("./chain_data", "/home/~/.epic/user/") {
			Ok(_) => {
				return;
			}
			Err(e) => panic!("{}", e),
		};
	}
}
