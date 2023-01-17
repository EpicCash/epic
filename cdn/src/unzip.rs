use std::fs;
use std::io;
use zip;

fn unzip() {
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
