# Epic Server - Build, Configuration, and Running

## Requirements

* rust 1.35
* clang
* ncurses and libs (ncurses, ncursesw5)
* zlib libs (zlib1g-dev or zlib-devel)
* pkg-config
* libssl-dev
* linux-headers (reported needed on Alpine linux)
* llvm

For Debian-based distributions (Debian, Ubuntu, Mint, etc), all in one line (except Rust):

```sh
apt install build-essential cmake git libgit2-dev clang libncurses5-dev libncursesw5-dev zlib1g-dev pkg-config libssl-dev llvm
```

For Mac using [brew](https://brew.sh/):

```sh
xcode-select --install
brew install --with-toolchain llvm
brew install pkg-config
brew install openssl
```

### Rust
Instructions of how to install rust can be found [here](https://www.rust-lang.org/tools/install).

During installation __rustup__ will attempt to configure the [__PATH__](https://en.wikipedia.org/wiki/PATH_(variable)). Because of differences between platforms, command shells, and bugs in __rustup__, the modifications to __PATH__ may not take effect until the console is restarted, or the user is logged out, or it may not succeed at all. **So, restart your console before proceeding to the next steps.**

After you have rust installed, execute the following command in the terminal:

```sh
rustup default 1.35.0
```

And then, check if you are using the correct version by typing the following command in the terminal:

```sh
rustc --version
```

The output should be something like this:

```sh
rustc 1.35.0 (3c235d560 2019-05-20)
```

## Build steps

```sh
git clone https://gitlab.com/epiccash/epic
cd epic
git submodule update --init --recursive
cargo build --release
```

Epic can also be built in debug mode (without the `--release` flag, but using the `--debug` or the `--verbose` flag) but this will render fast sync prohibitively slow due to the large overhead of cryptographic operations.

## What was built

A successful build gets you:

* `target/release/epic` - the main epic binary

## Running the Epic Server

If you want to execute the epic server, in the root directory of your Epic installation open a new terminal session and execute the following steps:

 1. Navigate to where your epic binary was generated using the followed command:

    ```sh
    cd target/release
    ```
 2. Configuring the __$PATH__ environment variable
 
     ```sh
    export LD_LIBRARY_PATH=$(find . -iname librandomx.so | head -n 1 | xargs dirname | xargs realpath)
    ```
 
 3. Execute the epic server using the following command:
  
    ```sh
    ./epic
    ```

**If the directory that you are starting the epic server doesn't have __epic-server.toml__ file, the epic server will be executed with the default file __~/.epic/main/epic-server.toml__.** More information can be found [here](./running.org#epic_config_default).

### (Optional) Adding Epic server to the path

The following steps describe how to execute epic from any location in **the current terminal session**:

 1. Open the terminal in the root directory of your Epic installation, and execute the following command to put the epic binary on your path:

    ```sh
    export PATH=`pwd`/target/release:$PATH
    ```

 2. After you set the path, you can run `epic` directly by typing in the terminal:

    ```sh
    epic
    ```

**If the directory that you are starting the epic server doesn't have __epic-server.toml__ file, the epic server will be executed with the default file __~/.epic/main/epic-server.toml__.** More information can be found [here](./running.org#epic_config_default).

## Mining in Epic

All mining functions for Epic are in a separate project called
[epic-miner](https://gitlab.com/epiccash/epic-miner).
