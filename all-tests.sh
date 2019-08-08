#!/bin/sh
cargo test --release
for i in $(find . -iname 'Cargo.toml' | grep -v progpow-rust | grep -v fuzz)
do
    cd $(dirname $i)
    cargo test --release
    cd -
done
