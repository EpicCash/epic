#!/bin/sh

for i in $(find . -iname '*.toml' | grep -v progpow-rust)
do
    cd $(dirname $i)
    cargo test --release
    cd -
done
