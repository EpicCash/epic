extern crate bindgen;
extern crate cmake;
extern crate filetime;

use filetime::FileTime;

use std::env;

use std::fs;

pub fn fail_on_empty_directory(name: &str) {
	if fs::read_dir(name).unwrap().count() == 0 {
		println!(
			"The `{}` directory is empty. Did you forget to pull the submodules?",
			name
		);
		println!("Try `git submodule update --init --recursive`");
		panic!();
	}
}

fn generate_bindings(out_dir: &str) {
	let bindings = bindgen::Builder::default()
		.header("randomx/src/randomx.h")
		.blacklist_type("max_align_t")
		.blacklist_type("_bindgen_ty_1")
		.generate()
		.expect("Unable to generate bindings");

	//let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
	bindings
		.write_to_file(format!("{}/ffi.rs", out_dir))
		.expect("Couldn't write bindings!");
}

fn compile_cmake() {
	cmake::Config::new("randomx").build_target("").build();
}

fn exec_if_newer<F: Fn()>(inpath: &str, outpath: &str, build: F) {
	if let Ok(metadata) = fs::metadata(outpath) {
		let outtime = FileTime::from_last_modification_time(&metadata);
		let intime = FileTime::from_last_modification_time(
			&fs::metadata(inpath).expect(&format!("Path {} not found", inpath)),
		);
		let buildfiletime =
			FileTime::from_last_modification_time(&fs::metadata("build.rs").unwrap());
		if outtime > intime && outtime > buildfiletime {
			return;
		}
	}
	build();
}

fn main() {
	println!("Starting randomx build");

	//generate_bindings();
	let out_dir = env::var("OUT_DIR").unwrap();

	fail_on_empty_directory("randomx");

	exec_if_newer("randomx", &format!("{}/build", out_dir), compile_cmake);

	exec_if_newer("randomx", &format!("{}/ffi.rs", out_dir), || {
		generate_bindings(&out_dir);
	});

	println!("cargo:rustc-link-search={}/build", out_dir);
	println!("cargo:rustc-link-lib=randomx");
}
