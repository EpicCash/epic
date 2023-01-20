![epic-logo-big-text-epic](https://user-images.githubusercontent.com/68653689/209132553-9c449c25-dbbf-4259-8456-87836f16b1c2.png)

[![Documentation Wiki](https://img.shields.io/badge/doc-wiki-blue.svg)](https://github.com/EpicCash/documentation/wiki)
[![Release Version](https://img.shields.io/github/release/EpicCash/epic.svg)](https://github.com/EpicCash/epic/releases/)
[![License](https://img.shields.io/github/license/EpicCash/epic.svg)](https://github.com/EpicCash/epic/blob/master/LICENSE)

This is the implementation of the epic server. The epic server is a node in the network that validates, propagates, and sometimes produces new blocks, basically a collection of processed transactions.

# Introduction to MimbleWimble and Epic

MimbleWimble is a blockchain format and protocol that provides extremely good scalability, privacy and fungibility by relying on strong cryptographic primitives. It addresses gaps existing in almost all current blockchain implementations.

Epic is an open source software project that implements a MimbleWimble blockchain and fills the gaps required for a full blockchain and cryptocurrency deployment.

The main goal and characteristics of the Epic project are:

* Privacy by default. This enables complete fungibility without precluding
  the ability to selectively disclose information as needed.
* Scales mostly with the number of users and minimally with the number of
  transactions (<100 byte `kernel`), resulting in a large space saving compared
  to other blockchains.
* Strong and proven cryptography. MimbleWimble only relies on Elliptic Curve
  Cryptography which has been tried and tested for decades.
* Design simplicity that makes it easy to audit and maintain over time.
* Community driven, encouraging mining decentralization.

## Status

Epic is live with mainnet.

# Getting Started (from 3.3.2 forward)

The full Epic Wiki can be found here: [Epic Cash - Wiki](https://github.com/EpicCash/documentation/wiki)

## Getting started with the project :bulb:

By the end of this section, you should have the basic knowledge of how to run Epic Cash and its different binaries functions.

Here are the basic topics:

- [Running the server](https://github.com/EpicCash/documentation/wiki/Running-the-server)
- [Running the wallet](https://github.com/EpicCash/documentation/wiki/Running-the-wallet)
- [Mining](https://github.com/EpicCash/documentation/wiki/Mining)

## Quick User guides :books: 

Has more information about the project, such as how to do transactions, and details about mining.

Here are the basic topics:
- [Epic wallet](https://github.com/EpicCash/documentation/wiki/Epic-wallet)
- [Epic miner](https://github.com/EpicCash/documentation/wiki/Epic-miner)

## Building the projects :toolbox:

If you want to build the projects, you should be able to have the minimum requirements for building the projects directly from their repositories.

This section is divided by OS:

- [Linux](https://github.com/EpicCash/documentation/wiki/Linux)
- [Windows](https://github.com/EpicCash/documentation/wiki/Windows)
- [macOS](https://github.com/EpicCash/documentation/wiki/macOS)

# Contributing :bricks: 

If you want to help us and contribute with our code:
- [Contributing](https://github.com/EpicCash/documentation/wiki/Contributing)

# Credits

Tom Elvis Jedusor for the first formulation of MimbleWimble.

Andrew Poelstra for his related work and improvements.

John Tromp for the Cuckoo Cycle proof of work.

Grin Developers for the initial implementation

J.K. Rowling for making it despite extraordinary adversity.

# License

Apache License v2.0.
