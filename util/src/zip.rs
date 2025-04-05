// Copyright 2018 The Grin Developers
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

use std::fs::{self, File};
/// Wrappers around the `zip-rs` library to compress and decompress zip archives.
use std::io::{self, BufReader, BufWriter, Write};
use std::panic;
use std::path::{Path, PathBuf};
use std::thread;
use walkdir::WalkDir;

use self::zip_rs::result::{ZipError, ZipResult};

use self::zip_rs::write::FileOptions;
use zip as zip_rs;

// Sanitize file path for normal components, excluding '/', '..', and '.'
// From private function in zip crate
fn path_to_string(path: &std::path::Path) -> String {
	let mut path_str = String::new();
	for component in path.components() {
		if let std::path::Component::Normal(os_str) = component {
			if !path_str.is_empty() {
				path_str.push('/');
			}
			path_str.push_str(&*os_str.to_string_lossy());
		}
	}
	path_str
}

/// Create a zip archive from source dir and list of relative file paths.
/// Permissions are set to 644 by default.
pub fn create_zip(dst_file: &File, src_dir: &Path, files: Vec<PathBuf>) -> io::Result<()> {
	let mut writer = {
		let zip = zip_rs::ZipWriter::new(dst_file);
		BufWriter::new(zip)
	};

	let options: FileOptions = FileOptions::default()
		.compression_method(zip_rs::CompressionMethod::Stored)
		.unix_permissions(0o644);

	for x in &files {
		let file_path = src_dir.join(x);
		if let Ok(file) = File::open(file_path.clone()) {
			info!("compress: {:?} -> {:?}", file_path, x);
			writer.get_mut().start_file(path_to_string(x), options)?;
			io::copy(&mut BufReader::new(file), &mut writer)?;
			// Flush the BufWriter after each file so we start then next one correctly.
			writer.flush()?;
		}
	}

	writer.get_mut().finish()?;
	dst_file.sync_all()?;
	Ok(())
}

/// Extract a set of files from the provided zip archive.
pub fn extract_files(from_archive: File, dest: &Path, files: Vec<PathBuf>) -> io::Result<()> {
	let dest: PathBuf = PathBuf::from(dest);
	let files: Vec<_> = files.iter().cloned().collect();
	let res = thread::spawn(move || {
		let mut archive = zip_rs::ZipArchive::new(from_archive).expect("archive file exists");
		for x in files {
			if let Ok(file) = archive.by_name(x.to_str().expect("valid path")) {
				let path = dest.join(file.mangled_name());
				let parent_dir = path.parent().expect("valid parent dir");
				fs::create_dir_all(&parent_dir).expect("create parent dir");
				let outfile = fs::File::create(&path).expect("file created");
				io::copy(&mut BufReader::new(file), &mut BufWriter::new(outfile))
					.expect("write to file");

				info!("Extract files: {:?} -> {:?}", x, path);

				// Set file permissions to "644" (Unix only).
				#[cfg(unix)]
				{
					use std::os::unix::fs::PermissionsExt;
					let mode = PermissionsExt::from_mode(0o644);
					fs::set_permissions(&path, mode).expect("set file permissions");
				}
			}
		}
	})
	.join();

	// If join() above is Ok then we successfully extracted the files.
	// If the result is Err then we failed to extract the files.
	res.map_err(|e| {
		error!("failed to extract files from zip: {:?}", e);
		io::Error::new(io::ErrorKind::Other, "failed to extract files from zip")
	})
}

/// Compress a source directory recursively into a zip file.
/// Permissions are set to 644 by default to avoid any
/// unwanted execution bits.
pub fn compress(src_dir: &Path, dst_file: &File) -> ZipResult<()> {
	if !Path::new(src_dir).is_dir() {
		return Err(ZipError::Io(io::Error::new(
			io::ErrorKind::Other,
			"Source must be a directory.",
		)));
	}

	let options = FileOptions::default()
		.compression_method(zip_rs::CompressionMethod::Stored)
		.unix_permissions(0o644);

	let mut zip = zip_rs::ZipWriter::new(dst_file);
	let walkdir = WalkDir::new(src_dir.to_str().unwrap());
	let it = walkdir.into_iter();

	for dent in it.filter_map(|e| e.ok()) {
		let path = dent.path();
		let name = path
			.strip_prefix(Path::new(src_dir))
			.unwrap()
			.to_str()
			.unwrap();

		if path.is_file() {
			zip.start_file(name, options)?;
			let mut f = File::open(path)?;
			io::copy(&mut f, &mut zip)?;
		}
	}

	zip.finish()?;
	dst_file.sync_all()?;
	Ok(())
}

/// Decompress a source file into the provided destination path.
pub fn decompress<R, F>(src_file: R, dest: &Path, expected: F) -> ZipResult<usize>
where
	R: io::Read + io::Seek + panic::UnwindSafe,
	F: Fn(&Path) -> bool + panic::UnwindSafe,
{
	let mut decompressed = 0;

	// catch the panic to avoid the thread quit
	panic::set_hook(Box::new(|panic_info| {
		error!(
			"panic occurred: {:?}",
			panic_info.payload().downcast_ref::<&str>().unwrap()
		);
	}));
	let result = panic::catch_unwind(move || {
		let mut archive = zip_rs::ZipArchive::new(src_file)?;

		for i in 0..archive.len() {
			let mut file = archive.by_index(i)?;
			let san_name = file.mangled_name();
			if san_name.to_str().unwrap_or("").replace("\\", "/") != file.name().replace("\\", "/")
				|| !expected(&san_name)
			{
				info!(
					"ignoring a suspicious file: {}, got {:?}",
					file.name(),
					san_name.to_str()
				);
				continue;
			}
			let file_path = dest.join(san_name);

			if (&*file.name()).ends_with('/') {
				fs::create_dir_all(&file_path)?;
			} else {
				if let Some(p) = file_path.parent() {
					if !p.exists() {
						fs::create_dir_all(&p)?;
					}
				}
				let res = fs::File::create(&file_path);
				let mut outfile = match res {
					Err(e) => {
						error!("{:?}", e);
						return Err(zip::result::ZipError::Io(e));
					}
					Ok(r) => r,
				};
				io::copy(&mut file, &mut outfile)?;
				decompressed += 1;
			}

			// Get and Set permissions
			#[cfg(unix)]
			{
				use std::os::unix::fs::PermissionsExt;
				if let Some(mode) = file.unix_mode() {
					fs::set_permissions(
						&file_path.to_str().unwrap(),
						PermissionsExt::from_mode(mode),
					)?;
				}
			}
		}
		Ok(decompressed)
	});
	match result {
		Ok(res) => match res {
			Err(e) => Err(e.into()),
			Ok(_) => res,
		},
		Err(_) => {
			error!("panic occurred on zip::decompress!");
			Err(zip::result::ZipError::InvalidArchive(
				"panic occurred on zip::decompress",
			))
		}
	}
}
