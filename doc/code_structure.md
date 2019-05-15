# Epic code structure

Epic is built in [Rust](https://www.rust-lang.org/), a memory safe, compiled language. Performance critical parts like the Cuckoo mining algorithm are built as plugins, making it easy to swap between algorithm implementations for various hardware. Epic comes with CPU and experimental GPU support.

## Files in project root

List of files tracked in `git` and some files you'll create when you use epic.

- [CODE_OF_CONDUCT](../CODE_OF_CONDUCT.md) - How to behave if you want to participate. Taken from rust. Slightly modified.
- [CONTRIBUTING](../CONTRIBUTING.md) - How to help and become part of epic.
- [Cargo.toml](../Cargo.toml) and Cargo.lock (locally created, _not_ in git) - defines how to the project code is to be compiled and built
- [LICENSE](../LICENSE) - Apache 2.0 license
- [README](../README.md) - The first document you should read, with pointers to find more detail.
- [rustfmt.toml](../rustfmt.toml) - configuration file for rustfmt. Required before contributing _new_ code.

## Folder structure

After checking out epic, building and using, these are the folders you'll have:

- `api`\
 Code for ApiEndpoints accessible over REST.
- `chain`\
 The blockchain implementation. Accepts a block (see pipe.rs) and adds it to the chain, or reject it.
- `config`\
 Code for handling configuration.
- `core`\
 All core types: Hash, Block, Input, Output, and how to serialize them. Core mining algorithm, and more.
- `doc`\
 All documentation.
- `servers`\
 Many parts (adapters, lib, miner, seed, server, sync, types) that the `epic` server needs, including mining server.
- `keychain`\
 Code for working safely with keys and doing blinding.
- `p2p`\
 All peer to peer connection and protocol-related logic (handshake, block propagation, etc.).
- `pool`\
 Code for the transaction pool implementation.
- `server`\
 A folder you're [supposed to create](build.md), before starting your server: cd to project root; mkdir server; cd server; epic server start (or run) and it will create a subfolder .epic
  - `.epic`
    - `chain` - a database with the blockchain blocks and related information
    - `peers` - a database with the list of Epic peers you're connected to
    - `txhashset` - contains folders kernel, rangeproof and output that each have a pmmr_dat.bin
- `src`\
  Code for the `epic` binary.
- `store`\
  Data store - Epic uses near-zero-cost Rust wrapper around LMDB, key-value embedded data store.
- `target`\
  Where the epic binary ends up, after the compile and build process finishes.
- `util`\
  Low-level rust utilities.
- `wallet`\
  Simple command line wallet implementation. Will generate:
  - `wallet_data` - a database storing your "outputs", that once confirmed and matured, can be spent with the [`epic wallet send`] command. (locally created, *not* in git)
  - `wallet.seed` - your secret wallet seed. (locally created, *not* in git)

## epic dependencies

- [secp256k1](https://github.com/mimblewimble/rust-secp256k1-zkp)
  Integration and rust bindings for libsecp256k1, and some changes waiting to be upstreamed. Imported in util/Cargo.toml.
