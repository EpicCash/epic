#![no_main]
#[macro_use]
extern crate libfuzzer_sys;
extern crate epic_core;
extern crate epic_p2p;

use epic_core::ser;
use epic_p2p::msg::Shake;

fuzz_target!(|data: &[u8]| {
	let mut d = data.clone();
	let _t: Result<Shake, ser::Error> = ser::deserialize(&mut d);
});
