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



use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read};

fn main() {
    println!("cargo:warning=build.rs started");

    // don't fail the build if something's missing, may just be cargo release
	let _ = built::write_built_file_with_opts(
		Some(Path::new(env!("CARGO_MANIFEST_DIR"))),
		&Path::new(&env::var("OUT_DIR").unwrap()).join("built.rs"),
	);

    // Determine target directory and profile (debug/release)
    let target_dir = env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let build_dir = format!("{}/{}", target_dir, profile);

    let build_tor = env::var("CARGO_FEATURE_WITH_TOR").is_ok();
    if !build_tor {
        println!("cargo:warning=Tor download skipped (feature 'with-tor' not enabled).");
        println!("cargo:warning=Enable Tor download with: --features with-tor");
        return;
    }


    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
     let tor_version = fs::read_to_string("tor-version.txt")
        .unwrap_or_else(|_| "14.5.4".to_string())
        .trim()
        .to_string();
    println!("cargo:rustc-env=TOR_VERSION={}", tor_version);
    let baseurl = format!(
        "https://archive.torproject.org/tor-package-archive/torbrowser/{}/",
        tor_version
    );

    // Map Rust target to Tor's naming
    let (_os_str, _arch_str, bundle_dir) = match (target_os.as_str(), target_arch.as_str()) {
        ("linux", "x86_64") => ("linux", "x86_64", "tor-expert-bundle-linux-x86_64"),
        ("macos", "x86_64") => ("macos", "x86_64", "tor-expert-bundle-macos-x86_64"),
        ("macos", "aarch64") => ("macos", "aarch64", "tor-expert-bundle-macos-aarch64"),     
        ("windows", "x86_64") => ("windows", "x86_64", "tor-expert-bundle-windows-x86_64"),
        _ => {
            println!("cargo:warning=No prebuilt Tor binary for this platform.");
            println!("cargo:warning=Please manually download the Tor expert bundle for your platform and extract it to the directory where Epic is located.");
            return;
        }
    };

    let archive_filename = format!("{}-{}.tar.gz", bundle_dir, tor_version);
    let archive_url = format!("{}{}", baseurl, archive_filename);
    let archive_path = format!("{}/{}", build_dir, archive_filename);

    // Download archive
    if !Path::new(&archive_path).exists() {
        println!("cargo:warning=Downloading Tor archive from {}", archive_url);
        let status = Command::new("curl")
            .args(&["-L", "-o", &archive_path, &archive_url])
            .status()
            .expect("Failed to run curl for archive");
        if !status.success() {
            panic!("Failed to download Tor archive");
        }
    } else {
        println!("cargo:warning=Tor archive already downloaded");
    }

    // SHA256 check
    let sha_file = "./etc/tor_signatures.txt";
    let expected_sha = get_expected_sha256(&archive_filename, sha_file)
        .unwrap_or_else(|| panic!("No SHA256 found for {} in {}", archive_filename, sha_file));
    verify_sha256(&archive_path, &expected_sha);

    // Prepare temp extraction directory
    let tmp_extract_dir = format!("{}/tmp_tor_extract", build_dir);
    if Path::new(&tmp_extract_dir).exists() {
        fs::remove_dir_all(&tmp_extract_dir).expect("Failed to clean old tmp_tor_extract");
    }
    fs::create_dir_all(&tmp_extract_dir).expect("Failed to create tmp_tor_extract");

    // Extract only the tor directory from the archive
    println!("cargo:warning=Extracting tor directory...");
    let status = Command::new("tar")
        .args(&["xf", &archive_path, "-C", &tmp_extract_dir, "tor"])
        .status()
        .expect("Failed to extract tor directory from archive");
    if !status.success() {
        panic!("Failed to extract tor directory from archive");
    }

    // Copy the tor directory to ./target/debug/tor or ./target/release/tor
    let extracted_tor_dir = format!("{}/tor", tmp_extract_dir);
    let dest_tor_dir = format!("{}/tor", build_dir);

    println!("cargo:warning=Copying Tor folder from {} to {}", extracted_tor_dir, dest_tor_dir);

    // Define tor_bin so it's available for later use (e.g., codesigning)
    let tor_bin = if target_os == "windows" {
        format!("{}/tor.exe", dest_tor_dir)
    } else {
        format!("{}/tor", dest_tor_dir)
    };

    if Path::new(&extracted_tor_dir).exists() {
        // Remove old tor dir if exists
        let _ = fs::remove_dir_all(&dest_tor_dir);
        copy_dir_all(&extracted_tor_dir, &dest_tor_dir).expect("Failed to copy Tor folder");
        println!("cargo:warning=Tor folder copied next to the Epic binary at: {}", dest_tor_dir);

        // Sign Tor binary on macOS before executing
        if target_os == "macos" {
            println!("cargo:warning=On macOS, all binaries and dylibs must be signed (at least ad-hoc) or execution will fail.");
            let sign_status = Command::new("codesign")
                .args(&["--force", "--deep", "--sign", "-", &tor_bin])
                .status();
            match sign_status {
                Ok(status) if status.success() => {
                    println!("cargo:warning=Tor binary signed with ad-hoc signature.");
                }
                Ok(status) => {
                    println!("cargo:warning=Failed to sign Tor binary, codesign exited with status: {}", status);
                }
                Err(e) => {
                    println!("cargo:warning=Failed to run codesign: {}", e);
                }
            }

            // Sign all .dylib files in the tor directory
            for entry in fs::read_dir(&dest_tor_dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "dylib" {
                        let sign_status = Command::new("codesign")
                            .args(&["--force", "--deep", "--sign", "-", path.to_str().unwrap()])
                            .status();
                        match sign_status {
                            Ok(status) if status.success() => {
                                println!("cargo:warning=Signed dylib: {}", path.display());
                            }
                            Ok(status) => {
                                println!("cargo:warning=Failed to sign dylib {}, codesign exited with status: {}", path.display(), status);
                            }
                            Err(e) => {
                                println!("cargo:warning=Failed to run codesign on dylib {}: {}", path.display(), e);
                            }
                        }
                    }
                }
            }

        }

        // Try to execute tor --version
        if Path::new(&tor_bin).exists() {
            let output = Command::new(&tor_bin)
                .arg("--version")
                .output();
            match output {
                Ok(out) => {
                    if out.status.success() {
                        let version = String::from_utf8_lossy(&out.stdout);
                        println!("cargo:warning=Tor binary executed successfully, version output:");
                        println!("cargo:warning={}", version.trim());
                    } else {
                        let err = String::from_utf8_lossy(&out.stderr);
                        println!("cargo:warning=Failed to run tor --version: {}", err.trim());
                    }
                }
                Err(e) => {
                    println!("cargo:warning=Error executing tor binary: {}", e);
                }
            }
        } else {
            println!("cargo:warning=Tor binary not found at {}", tor_bin);
        }
    } else {
        println!("cargo:warning=Listing tmp_extract_dir after extraction:");
        for entry in fs::read_dir(&tmp_extract_dir).unwrap() {
            println!("cargo:warning=  {:?}", entry.unwrap().path());
        }
        panic!("Tor folder not found after extraction!");
    }

    // Clean up temp extraction directory
    let _ = fs::remove_dir_all(&tmp_extract_dir);
}

// Recursively copy a directory
fn copy_dir_all(src: &str, dst: &str) -> std::io::Result<()> {
    let src_path = Path::new(src);
    let dst_path = Path::new(dst);
    fs::create_dir_all(dst_path)?;
    for entry in fs::read_dir(src_path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_entry_path = entry.path();
        let dst_entry_path = dst_path.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(src_entry_path.to_str().unwrap(), dst_entry_path.to_str().unwrap())?;
        } else {
            fs::copy(&src_entry_path, &dst_entry_path)?;
        }
    }
    Ok(())
}

// Read expected SHA256 from tor_signatures.txt
fn get_expected_sha256(archive_filename: &str, sigfile: &str) -> Option<String> {
    let file = fs::File::open(sigfile).ok()?;
    for line in BufReader::new(file).lines() {
        let line = line.ok()?;
        let mut parts = line.split_whitespace();
        let name = parts.next()?;
        let sha = parts.next()?;
        if name == archive_filename {
            return Some(sha.to_string());
        }
    }
    None
}

// Compute and verify SHA256
fn verify_sha256(path: &str, expected: &str) {
    let mut file = fs::File::open(path).expect("Failed to open file for SHA256 check");
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf).unwrap();
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        panic!("SHA256 mismatch: expected {}, got {}", expected, actual);
    } else {
        println!("cargo:warning=SHA256 checksum verified");
    }
}
