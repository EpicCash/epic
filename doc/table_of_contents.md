# Documentation structure

## Explaining epic

- [intro](intro.md) - Technical introduction to epic
- [epic4bitcoiners](epic4bitcoiners.md) - Explaining epic from a bitcoiner's perspective

## Understand the epic implementation

- [chain_sync](chain/chain_sync.md) - About how Epic's blockchain is synchronized
- [blocks_and_headers](chain/blocks_and_headers.md) - How Epic tracks blocks and headers on the chain
- [contract_ideas](contract_ideas.md) - Ideas on how to implement contracts
- [dandelion/dandelion](dandelion/dandelion.md) - About transaction propagation and cut-through. Stemming and fluffing!
- [dandelion/simulation](dandelion/simulation.md) - Dandelion simulation - aggregating transaction without lock_height Stemming and fluffing!
- [internal/pool](internal/pool.md) - Technical explanation of the transaction pool
- [merkle](merkle.md) - Technical explanation of epic's favorite kind of merkle trees
- [merkle_proof graph](merkle_proof/merkle_proof.png) - Example merkle proof with pruning applied
- [pruning](pruning.md) - Technical explanation of pruning
- [stratum](stratum.md) - Technical explanation of Epic Stratum RPC protocol
- [transaction UML](wallet/transaction/basic-transaction-wf.png) - UML of an interactive transaction (aggregating transaction without `lock_height`)

## Build and use

- [api](api/api.md) - Explaining the different APIs in Epic and how to use them
- [build](build.md) - Explaining how to build and run the Epic binaries
- [release](release_instruction.md) - Instructions of making a release
- [usage](usage.md) - Explaining how to use epic in Testnet3
- [wallet](wallet/usage.md) - Explains the wallet design and `epic wallet` sub-commands
