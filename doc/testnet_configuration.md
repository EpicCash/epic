# Epic Server - Floonet Configuration

## Objective

This document aims to show you how to become a contributor in the testing area and help Epic to be an increasingly great cryptocurrency.

## Requirements

Before starting the settings, we first need to have installed the most current version of Epic's code on GitHub. 

To do this, just follow what is described in [README.md](https://github.com/EpicCash/epic/#readme) or in [build.md](https://github.com/EpicCash/epic/blob/master/doc/build.md).

Once you have all Node, Wallet and Miner services up and running, just follow this document to configure your environment to connect to the test environment.

## How to Access the Testnet

After having to download the latest version of the Epic Server, Wallet, and Miner. You need following the steps:

### Reset the Server Configuration

If the testnet is restarted or there's a new version of the epic server, you will need to remove the directory called **/chain/data**. This directory is where the epic cash blockchain stores its data. 

Therefore, if the testnet is restarted, all this data needs to be removed in order to run and store the newest version of the blockchain. The following steps explain how to erase this data using the terminal:

1. Open a new terminal window in the directory where you saved the epic server data. If you used the [default configuration](./running.org#epic_config_default), this folder should be under **~/.epic/floo** in you home directory.
2. Then execute the following command:

   ```sh
   rm -rf chain_data/
   ```
After that check, if you have the following settings for your node to connect to the other floonet servers:

#### Windows

Download all binaries from the requirements list above and extract them in a convenient location (e.g C:\Program Files\Epic).

1. Go to the directory to store the configuration files:

  Using PowerShell:

  ```shell
  cd $HOME\.epic\floo
  ```

  Using CMD:

  ```cmd
  cd $USERPROFILE%\.epic\floo
  ```

2. Open the `epic-server.toml` file and change the value of `seeding_type` from `DNSSeed` to `List`:

  ```toml
  #how to seed this server, can be None, List or DNSSeed
  seeding_type = "List"
  ```

4. And add the following peers to the seeds list:

  ```toml
  #If the seeding type is List, the list of peers to connect to can
  #be specified as follows:
  seeds = ["15.228.37.137:13414", "3.223.249.67:13414"]
  ```
  Save the `epic-server.toml` file.

5. Start the server and wait until it is fully synchronized with the other peers:

  ```shell
  epic.exe --floonet
  ```

#### Linux

These are the steps to reset your `floonet`:

1. Go to the directory to store the configuration files:

  ```bash
  cd ~/.epic/floo
  ```

2. Open the `epic-server.toml` file and change the value of `seeding_type` from `DNSSeed` to `List`:

  ```toml
  #how to seed this server, can be None, List or DNSSeed
  seeding_type = "List"
  ```

3. And add the following peers to the seeds list:

  ```toml
  #If the seeding type is List, the list of peers to connect to can
  #be specified as follows:
  seeds = ["15.228.37.137:13414", "3.223.249.67:13414"]
  ```
  Save the `epic-server.toml` file.

4. Start the server and wait until it is fully synchronized with the other peers:

  ```bash
  epic --floonet
  ```

### First Run of Epic Server

#### Windows

Download all binaries from the requirements list above and extract them in a convenient location (e.g C:\Program Files\Epic).

1. Create the directory to store the configuration files:

  Using PowerShell:

  ```shell
  md -f $HOME\.epic\floo

  cd $HOME\.epic\floo
  ```

  Using CMD:

  ```cmd
  md $USERPROFILE%\.epic\floo

  cd $USERPROFILE%\.epic\floo
  ```

2. Create the server configuration files:

  ```shell
  epic.exe --floonet server config
  ```

3. Open the `epic-server.toml` file and change the value of `seeding_type` from `DNSSeed` to `List`:

  ```toml
  #how to seed this server, can be None, List or DNSSeed
  seeding_type = "List"
  ```

4. And add the following peers to the seeds list:

  ```toml
  #If the seeding type is List, the list of peers to connect to can
  #be specified as follows:
  seeds = ["15.228.37.137:13414", "3.223.249.67:13414"]
  ```
  Save the `epic-server.toml` file.

5. Create the flag file:

  Using PowerShell:

  ```shell
  New-Item 2-15-rollback-flag
  ```

  Using cmd:

  ```shell
  type nul > 2-15-rollback-flag
  ```

6. Start the server and wait until it is fully synchronized with the other peers:

  ```shell
  epic.exe --floonet
  ```

#### Linux

These are the steps to access the `testnet`:

1. Create the directory to store the configuration files:

  ```bash
  mkdir -p ~/.epic/floo && cd ~/.epic/floo
  ```

2. Create the server configuration file:

  ```bash
  epic --floonet server config
  ```

3. Open the `epic-server.toml` file and change the value of `seeding_type` from `DNSSeed` to `List`:

  ```toml
  #how to seed this server, can be None, List or DNSSeed
  seeding_type = "List"
  ```

4. And add the following peers to the seeds list:

  ```toml
  #If the seeding type is List, the list of peers to connect to can
  #be specified as follows:
  seeds = ["15.228.37.137:13414", "3.223.249.67:13414"]
  ```
  Save the `epic-server.toml` file.

5. Create the flag file:

  ```bash
  touch 2-15-rollback-flag
  ```

6. Start the server and wait until it is fully synchronized with the other peers:

  ```bash
  epic --floonet
  ```

## How to Start Mining on Testnet

### Windows

1. Still on the same `.epic/floo` directory, initiate the wallet:

  ```shell
  epic-wallet.exe --floonet init -h
  ```

  You'll be prompted to enter the password for the wallet.

2. Create a default account:

  ```shell
  epic-wallet.exe --floonet account
  ```

3. Double check the initial balance:

  ```shell
  epic-wallet.exe --floonet info
  ```

4. Start listening for connections on the `epic-wallet`:

  ```shell
  epic-wallet.exe --floonet listen
  ```

5. Configure your miner to work with the `floonet` server by changing the port of the stratum server on the `epic-miner.toml` file.
   This file is located in the folder where you extracted the `epic-miner` files.

  ```toml
  # listening epic stratum server url
  stratum_server_addr = "127.0.0.1:13416"
  ```

6. Open a new terminal, then go to the folder where you extracted the `epic-miner` and execute it:

  ```shell
  epic-miner.exe
  ```

  or

  ```shell
  epic-miner-opencl.exe
  ```

### Linux

1. Still on the same `.epic/floo` directory, initiate the wallet:

  ```bash
  epic-wallet --floonet init -h
  ```

  You'll be prompted to enter the password for the wallet.

2. Create a default account:

  ```bash
  epic-wallet --floonet account
  ```

3. Double check the initial balance:

  ```bash
  epic-wallet --floonet info
  ```

4. Start listening for connections on the `epic-wallet`:

  ```bash
  epic-wallet --floonet listen
  ```

5. Configure your miner to work with the `floonet` server by changing the port of the stratum server on the `/opt/epic-miner/epic-miner.toml` file:

  ```toml
  # listening epic stratum server url
  stratum_server_addr = "127.0.0.1:13416"
  ```

  The example above is related to the CPU-only miner.

6. Start the miner:

  ```bash
  epic-miner
  ```