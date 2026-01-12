# Dcoin

## Getting Started

1. Create a few wallets to scaffold the blockchain using the cli `create-wallet` fn

- `cargo run create-wallet`

2. Create the blockchain using the CLI with the `create-blockchain` command, passing in the wallet of your choice. This will mine the genesis block and provide a mining award to the given wallet.

- `cargo run create-blockchain -a <address>`

3. Start your node, passing the given p2p and rest api ports you'd like to provision. Optionally pass a wallet ID to recieve rewards for any mined txs.

- `cargo run start-node -p 4000 -r 3000`

## How to Use

cli

api
