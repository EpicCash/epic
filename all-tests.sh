#!/bin/sh
for i in $(find . -iname 'Cargo.toml' | grep -v progpow-rust | grep -v fuzz)
do
    echo $(dirname $i)
    cd $(dirname $i)
    cargo test --release
    cd -
done
