

# epic 3.0.0

  - Fix: Address issue Breaks windows build #50
  - Merge pull request #45 from acosta-brickabode/fix/log-level-syncer
  - Merge pull request #46 from acosta-brickabode/feature/gh-action-linux
  - Fix: Replace pancurses-backend feature with termion-backend to fix ncurses rendering issues
  - Refactor: Remove libssl-dev as build requirement
  - Fix: Replace log::info feature with log::debug to fix overprinting sync messages
  - Feat: Implement Github Action for generating Linux artifact
  - Doubling the value of the MAINNET_FIRST_HARD_FORK constant (#40)
  - Fix: Update croaring to a fork that doesn't use `-march=native` that was causing the illegal instruction errors
  - Retry connecting to the seed servers when there is no connected peers (#38)
  - Test(config): Add unit tests to check compatibility with v2 and v3 values for log-levels
  - Fix(config): Use custom deserialization function that accepts log-level values from both v2 and v3
  - Merge pull request #35 from EpicCash/fix/floonet-foundation
  - Fix(floonet): Make `foundation_floonet.json` the same as `foundation.json`
  - Merge pull request #34 from EpicCash/feature/create-block-fees-submodule
  - Move the BlockFeeds type a separate location for better reusage: the wallets can use it there
  - Increasing the FLOONET_FIRST_HARD_FORK to 25800
  - Merge pull request #32 from EricShimizuKarbstein/update-docs
  - Merge pull request #33 from EpicCash/fix/cmake-error
  - Update randomx-rust
  - Update build instructions
  - Fix: more chunk size for header sync
  - Fix: add missing shutdown info in cmd mode
  - Refactor: update zeroise macro keychain, utils
  - Update epic cargo docs and wrong comments in code #29
  - Fix: update some deps in keychain #28
  - Merge pull request #27 from hdmark/patch-1
  - Fix: update header sync; remove dublicate check bad header; remove debug output from graph weight
  - Fix: move reset stalling, sync peer reset
  - remove unused debug output
  - change header sync ban action
  - Fix: windows foundation height
  - Removing verifier caches #26
  - make small peers sync improvements
  - Fix: make header sync faster
  - update foundation sha for unix
  - update foundation hash; peers v2 to v3 sync
  - P2P: better handling of mpsc channel and initial handshake messages with other peers
  - Chain: Better output PMMR handling and Chain DB index. Improve stability and DB crashes.
  - New Header Version 7 and better Header Version Switch
  - Tui:
    - Some TUI improvements.
    - Lists can be scrolled now.
  - Split of File and Console  output and cleanup
  - API:
    - API enhancements and new API v3 for Wallet communication


# epic (3.0.0-beta)

  - changing algo mix for next block era 3
  - split stdout logformat from file log; stdout cosmetics


# epic (3.0.0-alpha-6)

  - rewind #3045
  - rewind Read header_head and sync_head from header MMRs directly
  - addd missing chain setup
  - additional code changes v3
  - pipe changes
  - add header hash to header info
  - check store for block sums
  - remove fixed size
  - change db store consts
  - add missing code v3
  - change zeroise to 1.3.x
  - change zerois to 1.x
  - change floonet era height; fix MAINNET_DIFFICULTY_ERA height
  - change current header version to 6
  - change cursive version
  - alpha6 finalize for testing
  - change stratum rpc id value
  - change floonet seed
  - fix v3 #3268


# epic (3.0.0-alpha-5)

  - change floonet first hardfork to header version 7 to blockheight 5760
  - change header back to v7
  - fix windows compatibility
  - add badblock script from 2.15;cursive tui update;version checker; set to v3.0.0
  - update ser because of serde::export private
  - Merge pull request #15 from johanneshahn/develop
  - Update build.md
  - Update build.md
  - Add missing 222 to FOUNDATION_LEVY
  - add missing foundation levy 222
  - API client basic authorization.
  - Adding cucumber tests - trying to spend foundation coins
  - Merge pull request #7 from EpicCash/fix-foudation-levy


# epic (3.0.0-alpha-4)

  - Merge branch 'fix-emission-schedule' into 'develop'
  - Update emission schedule
  - Merge branch 'fix--duration-tui' into 'develop'
  - Fix wrong duration on tui
  - Merge branch 'fix-floonet-foundation' into 'develop'
  - Fix floonet foundation on windows


# epic (3.0.0-alpha-3)

  - Fixing the communication between server and wallet
  - Update build.md -> remove "brew install llvm" for build with Mac. Mac ships own llvm


# epic (3.0.0-alpha-2)

  - Fixing conflict with EpicWallet


# epic (3.0.0-alpha-1)

  - This patch only affects floonet
  - Integrates grin 3.0.0 codebase into ours
  - Fixes some synchronization problems
  - Adjusts the difficulty adjustment a bit
  - Introduces a hard-fork in floonet at height 2880

# epic (3.0.0-alpha)

- Merge branch 'develop'
- Merge branch '1st-hard-fork' into develop
- Add foundation for floonet
- Add height for mainnet hardfork
- Fix load foundation for differents version
- Fix display of the difficulty
- Fixing some core tests.
- Add build foundation command
- Fix p2p and wallet signature
- Update package version
- Fix txhashset keeping with invalid block hash
- Fix legacy transactions on cucumber tests
- Fix many cucumber tests
- Every test is compiling now.
- Fixing loading of previous block
- Compiling pool.
- Compiling core tests.
- Fixing compilation for cucumber tests
- Fixing compilation for chain tests.
- Fixing API tests.
- Fix log
- Update tui
- Add function missing on core
- Update server version
- Update api version
- p2p and pool now compiling.
- epic_core is now compiling.
- epic_util should be compiling.
- epic_store should be compiling.
- epic_core should be compiling.
- Fast Sync initial files
- Merge branch 'update-floonet' into 'develop'
- Merge branch 'update_macos_build_instructions' into 'develop'
- Modify files for first Hard Fork
- Update Floonet DNS seeds


# epic (2.4.0)

  - Add 240 foundation slips


# epic (2.3.1-1)

  - Make header sync timeout user-configurable
  - Change the default header sync timeout
  - Improve the broadcasting of mined blocks


# epic (2.3.0-2)

  - Fix package generation on rust 1.36.0
  - Updating submodules before building


# epic (2.3.0-1)

  - Update documentation
  - Fix epic-server link
  - Add windows documentation
  - Fix sync threshold bug
  - Fix sync difficulty bug


# epic (2.2.3-1)

  - Update dns


# epic (2.2.2-1)

  - Release version


# epic (2.1.0-1)

  - Changing the genesis block


# epic (2.0.0-1)

  - Fix stratum server overflow
  - Recovering the old copyright notices
  - Official foundation.json file.
  - Add sha256 verification to the foundation.json
  - Updating the sha256sum with the new foundation.json
  - Increase block version


# epic (1.6.0-1)

  - Fix test for accept pow
  - Change the timestamp computation
  - Improve the readability of cucumber tests
  - Remove unnecessary prints
  - Generate new timestamp for each job
  - Fix some tests
  - Decrease the control constant of progpow
  - Increase header version to 4
  - Fix node sending double jobs for miner
  - Fix load fundation in the windows
  - Fix cursive for windows and foundation path
  - Add tests for load foundation file
  - Fix mining duration time in TUI
  - Update the foundation rewards
  - Add assert to invalid heights
  - Fix more display errors in the TUI
  - Update foundation.json
  - Fix genesis test
  - Fix select fork with difficulty
  - Fix infinite loop in the cucumber test
  - Add tests for fork based in the difficulty
  - Fix some cucumber errors
  - Add scripts for windows installer
  - Update progpow and randomx version
  - Fix installer version
  - Fix select fork with difficulty
  - Fix infinite loop in the cucumber test
  - Add tests for fork based in the difficulty
  - Fix some cucumber errors
  - Add current difficulty in the job template
  - Change develop version
  - Fix ProgPow average not being displayed
  - Fix tests with invalid pow


# epic (1.5.0-1)

  - Reenable Continuous Integration.
  - Adding a cache for faster builds.
  - Remove md5 test and fix some proofs
  - Share the cache between all branches.
  - Fixing genesis hash in tests.
  - New cache configuration.
  - More genesis hash fixes.
  - Update documentation and change some defaults
  - Update submodules.
  - Add next algorithm in the job template
  - Add cucumber difficulty adjustment tests
  - Add more tests for multi algo difficulty adjustment
  - Change cuckoo initial difficulty
  - update randomx rust
  - Fix all tests
  - Change block version


# epic (1.4.0-2)

  - Fixing postinst script


# epic (1.4.0-1)

  - Fix extra time for difficulty
  - Add const global for change block version
  - Change version serialize
  - Changing the directory of foundation.json
  - Add dns version checker
  - Add version struct
  - Add version.rs file
  - Improve the error message


# epic (1.3.1-1)

  - Fix dns address


# epic (1.3.0-1)

  - Add multi-gpu mining instructions
  - Fix seed validation when it is synchronizing
  - Fix cuckatoo reversing difficulty of progpow and randomx
  - Improving dpkg documentation.
  - Add floonet dns seed


# epic (1.2.0-1)

  - Fix almost all grin tests
  - Fix indentation
  - Add cargo lock for p2p fuzz tests
  - Fix all no-cucumber tests
  - Add support for multi policy
  - Add cursor for search block with same policy and update config
  - A bug report tool for Epic.
  - Adding (untested) Windows commands to get cpuinfo and lspci.
  - Reduce the height for coinbase maturity in the floonet
  - Add randomx seed changing in each epochs
  - Fix validation and load multiples epochs
  - Remove conflict tag
  - Remove condition repeated
  - Change the rewards to match the whitepaper
  - Fix non-cucumber tests
  - Improve cucumber tests names
  - Add foundation height for floonet
  - Increase floonet foundation height
  - Remove genesis reward
  - Fix genesis hash test
  - Update randomx rust
  - Fix invalid seed in tests
  - Adding epic-bugreport to the package.
  - Add policies matching the white paper
  - Fix calc for seed height
  - change randomx epochs
  - Aborting the package build in case of an error.
  - Change the default min share difficulty
  - Fix diff factor for randomx and decrease progpow difficulty


# epic (1.1.0-1)

  - Documentation on how to submit bug reports.
  - Adjust build block with first block timestamp
  - Increase progpow difficulty and diff factor
  - Run all tests script.
  - Increase progpow difficulty
  - Adjust difficulty


# epic (1.0.5-1)

  - Fix ovefflow in difficultyiter


# epic (1.0.4-1)

  - Update randomx rust
  - Fix pow rejected
  - Fix Randomx control algo
  - Fix selection of algo in blockchain
  - Update the selection of timestamps from the blockchain
  - Fix cargo build
  - Correct difficulty difference
  - Fix calc for differents timestamp
  - Fix get list of old difficulty
  - Update the link to the packages
  - Fix total difficulty calc
  - Fix return of prev header when none
  - Fix interator prev
  - Fix i64 values in the DifficultyIter


# epic (1.0.3-3)

  - Fixing the doc for testnet reset.
  - It's chain_data, not /chain/data
  - Todd's review of the testnet reset doc.
  - Update testnet reset instructions
  - Add specific height for foundation reward
  - Add mine using only cpu
  - Add mine using only cpu
  - Update links and checksum
  - Fix the height sent to the wallet API
  - Update progpow rust
  - Update RandomX config
  - Installing foundation.json with write permissions.
  - Maintainance script for epic.


# epic (1.0.2-1)

  - Fixing bugs with progpow


# epic (1.0.0-2)

  - Fixing the dependencies
  - Removing the pipe from the network


# epic (1.0.0-1)

  - Initial release
