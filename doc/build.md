# Epic Server - Build, Configuration, and Running

## Requirements

- rust 1.56
- clang
- ncurses and libs (ncurses, ncursesw5)
- zlib libs (zlib1g-dev or zlib-devel)
- pkg-config
- linux-headers (reported needed on Alpine linux)
- llvm (which can be downloaded [here](https://github.com/llvm/llvm-project/releases))
- cmake (tested with versions 3.18.4 and 3.19.3)

For Debian-based distributions (Debian, Ubuntu, Mint, etc), all in one line
(except Rust):

```sh
sudo apt install build-essential cmake git libgit2-dev clang libncurses5-dev libncursesw5-dev zlib1g-dev pkg-config llvm
```

For Mac using [brew](https://brew.sh):

```sh
brew install pkg-config
brew install openssl
brew install cmake
brew install rustup
brew install coreutils
```

### Rust

Instructions of how to install rust can be found
[here](https://www.rust-lang.org/tools/install).

During installation **rustup** will attempt to configure the
[**PATH**](https://en.wikipedia.org/wiki/PATH_(variable)). Because of
differences between platforms, command shells, and bugs in **rustup**, the
modifications to **PATH** may not take effect until the console is restarted, or
the user is logged out, or it may not succeed at all. **So, restart your console
before proceeding to the next steps.**

After you have rust installed, execute the following command in the terminal:

```sh
rustup default 1.56.0
```

And then, check if you are using the correct version by typing the following
command in the terminal:

```sh
rustc --version
```

The output should be something like this:

```sh
rustc 1.56.0 (xxxxx)
```


## Build steps

```sh
git clone https://github.com/EpicCash/epic
cd epic
git submodule update --init --recursive
cargo update
cargo build --release
```

Epic can also be built in debug mode (without the `--release` flag, but using
the `--debug` or the `--verbose` flag) but this will render fast sync
prohibitively slow due to the large overhead of cryptographic operations.

## What was built

A successful build gets you:

- `target/release/epic` - the main epic binary

## Running the Epic Server

If you want to execute the epic server, in the root directory of your Epic
installation open a new terminal session and execute the following steps:

1. Navigate to where your epic binary was generated using the followed command:

   ```sh
   cd target/release
   ```

2. <s>Configuring the **$PATH** environment variable</s>


   ```sh
   export LD_LIBRARY_PATH=$(find . -iname librandomx.so | head -n 1 | xargs dirname | xargs realpath)
   ```

   <s>For MacOs use (need brew coreutils to be installed):</s>

   ```sh
   export LD_LIBRARY_PATH=$(find . -iname librandomx.dylib | head -n 1 | xargs dirname | xargs realpath)
   ```
   
   
3. Create the epic-server.toml config and Init the epic server using the following command:
   ```sh
   ./epic server config
   ```
4. Execute the epic server using the following command:

   ```sh
   ./epic
   ```

**If the directory that you are starting the epic server doesn't have
**epic-server.toml** file, the epic server will be executed with the default
file **~/.epic/main/epic-server.toml**.** More information can be found on the
topic [Configuring your Epic node](./running.org#epic_config_default).

### (Optional) Adding Epic server to the path

The following steps describe how to execute epic from any location in **the
current terminal session**:

1. Open the terminal in the root directory of your Epic installation, and
   execute the following command to put the epic binary on your path:

   ```sh
   export PATH=`pwd`/target/release:$PATH
   ```

2. After you set the path, you can run `epic` directly by typing in the
   terminal:

   ```sh
   epic
   ```

**If the directory that you are starting the epic server doesn't have
**epic-server.toml** file, the epic server will be executed with the default
file **~/.epic/main/epic-server.toml**.** More information can be found on the
topic [Configuring your Epic node](./running.org#epic_config_default).

## Mining in Epic

All mining functions for Epic are in a separate project called
[epic-miner](https://gitlab.com/epiccash/epic-miner).

<a id="testnet_reset"></a>

## How to Connect to Testnet

To connect to floonet properly, just follow the steps indicated in the file [testnet configuration](https://github.com/EpicCash/epic/blob/master/doc/testnet_configuration.md).

## Testnet Configuration

If you want to run one of the testnets (**floonet** or **usernet**) to try out new features and simulate distinct scenarios without using a real wallet, you need to add the parameter "--floonet" (remote testnet) or "--usernet" (local testnet, recommended) after the epic server and the epic wallet to be used.

   ```sh
   epic --usernet --onlyrandomx...
   epic-wallet --usernet ...
   ```
   
Floonet data is stored on the directory path **<HOME>/.epic/floo**, while usernet data is stored on the directory path **<HOME>/.epic/user**.

To be able to run a testnet properly, you need to ensure that:

1. When running the epic-miner, the **stratum_server_addr** variable available in the file epic-miner.toml should be 13416;
2. When running the epic-server for floonet, the file epic-server.toml must be configured so that the **seeding_type = List** and the **seeds = ["15.229.31.27:23414"]**;
3. When running the epic-server for usernet, the file epic-server.toml must be configured so that the **stratum_server_addr = "127.0.0.1:13416"**;

Also, be careful with the epic-wallet.toml file that may be downloaded from the repository, because if this file is available in the same folder as the epic-wallet executable, 
it will be used by the application rather than the epic-wallet.toml file available in the home path.
   
## Testnet Reset

If the testnet is restarted or there's a new version of the epic server, you
will need to remove the directory called **/chain/data**. This directory is
where the epic cash blockchain stores its data. Therefore, if the testnet is
restarted, all this data needs to be removed in order to run and store the
newest version of the blockchain. The following steps explain how to erase this
data using the terminal:

1. Open a new terminal window in the directory where you saved the epic server
   data. If you used the
   [default configuration](./running.org#epic_config_default), this folder
   should be under **~/.epic/main** in you home directory.
2. Then execute the following command:

   ```sh
   rm -rf chain_data/
   ```

## Building the debian packages

Deb package is binary-based package manager. We have build scripts .deb packages
in the following repos:

- RandomX
- Epic
- Epic Wallet
- Epic Miner

In order to build one, you need to first clone the repos:

```sh
git clone --recursive git@github.com:EpicCash/epic.git
git clone --recursive git@github.com:EpicCash/epic-wallet.git
git clone --recursive git@github.com:EpicCash/epic-miner.git
git clone --recursive git@github.com:EpicCash/randomx.git
```

Then install all the package listed under the `Build-Depends` section in the
`debian/control` file of the respective repository. To be safe, these are all
the needed packages in all the repositories:

```sh
sudo apt-get install build-essential debhelper cmake libclang-dev libncurses5-dev clang libncursesw5-dev cargo rustc opencl-headers libssl-dev pkg-config ocl-icd-opencl-dev
```

There's some special commands needed in order to install CUDA (which is
necessary for epic-miner-cuda). Follow the instructions in
[this link](https://developer.nvidia.com/cuda-downloads?target_os=Linux&target_arch=x86_64&target_distro=Ubuntu&target_version=1810&target_type=deblocal).

Finally, run from the respective project root the following command:

```sh
fakeroot make -f debian/rules binary
```

## Adjusting algorithm difficulties

In the next few days we will need to adjust the difficulties in order to reach
an ideal point. In order to change that manually access the file
**core/src/genesis.rs** from epic root directory. Look for the functions
**genesis_floo** and **genesis_main** and search for the lines that look like
the following:

```rust
diff.insert(PoWType::Cuckaroo, 2_u64.pow(1));
diff.insert(PoWType::Cuckatoo, 2_u64.pow(1));
diff.insert(PoWType::RandomX, 2_u64.pow(16));
diff.insert(PoWType::ProgPow, 2_u64.pow(8));
```

And change the values under **.pow()**.

After you did those things you will need to rebuild the package, the testnet and
everybody participating in the network will need to install the new package and
restart all the services. More instruction of how to that can be found in the
topic [Testnet Reset](#testnet_reset).
