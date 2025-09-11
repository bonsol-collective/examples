## Hash Locked Fund Escrow

This example demonstrates a hash locked escrow system on Solana, where funds are locked in an escrow and can only be claimed by providing the correct preimage for a stored hash.

### Components

- **data-server**: Node.js server providing a public endpoint that returns a 32-byte value as public input.
- **sha**: Bonsol application that computes the SHA-256 hash of the provided public input.
- **sol-program**: Solana program with three instructions:
	- `initialize_escrow`: Initializes an escrow and locks the specified amount of lamports.
	- `claim_escrow`: Allows a user to claim the escrow by submitting a preimage; requests the hash from the Bonsol program.
	- `handle_claim_callback`: Receives the hash from Bonsol and, if it matches the escrow's hash, releases funds to the receiver.
- **client**: TypeScript client to interact with the Solana program and call escrow instructions.

**Flow:**
1. Escrow is initialized with a hash and locked funds.
2. Anyone with the correct preimage can claim the escrow.
3. The claim process verifies the hash via Bonsol before releasing funds.

## Flow Diagram

```mermaid
graph LR
    A[Sender] -->|"1. Lock funds<br/>with hash"| B[Escrow Account]
    A -.->|"2. Share secret<br/>off-chain"| C[Receiver]
    C -->|"3. Submit<br/>pre-image"| D[Bonsol]
    D -->|"4. Calculate<br/>hash"| E{Hash<br/>Match?}
    B -.->|Stored hash| E
    E -->|"Yes ✓"| F[Funds Released]
    E -->|"No ✗"| G[Funds Locked]
    
    style A fill:#e0f2fe
    style C fill:#e0f2fe
    style D fill:#fef3c7
    style B fill:#fee2e2
    style F fill:#d1fae5
    style G fill:#fee2e2
    style E fill:#f3f4f6
```

## Process Steps

### 1. Create Escrow Account
Sender locks funds in an escrow account with SHA-256 hash of secret

### 2. Share Secret
Sender gives pre-image to receiver off-chain

### 3. Claim Funds
Receiver submits pre-image to unlock

### 4. Verify & Release
Bonsol computes hash, Solana verifies and releases

## Why Bonsol?

Bonsol enables off-chain SHA-256 computation with zero-knowledge proofs, allowing larger input sizes than native Solana constraints. The system ensures trustless verification: funds only release when the correct pre-image is provided.


**Setting Up Environment**
1. Navigate to the bonsol root directory:
	- Start the Local Validator
	```bash
	./bin/validator.sh
	```
	- Run the Bonsol Prover Node
	```bash
	./bin/run-node.sh
	```
	- Run the Local ZK Program Server
	```bash
	cargo run -p local-zk-program-server
	```
2. Navigate to `examples/sha_256/data-server` and start the data server:
	```bash
	npm install
	npm run start
	```
3. Navigate to `examples/sha_256/sha`, build and deploy the Bonsol application
	```bash
	bonsol build --zk-program-path .
	bonsol deploy url --url http://localhost:8080 --manifest-path manifest.json
	```
4. Navigate to `examples/sha_256/sol-program`, build and deploy the Sol
	- update the `SHA256_IMAGE_ID` with the response from previous step in `lib.rs` and `client.ts`
	- deploy the solana program
	```bash
	cargo build-sbf
	solana program deploy ./target/deploy/sol-program.so
	```
5. Navigate to `examples/sha_256/client`, install dependencies and run the client
	```bash
	npm install
	ts-node src/client.ts
	```
***NOTE***
- You need to configure/sync the output of `data-server` and `secret` in `client.ts` to successfully claim the escrow.
- For every new request, you need to update `executionId` in `client.ts` to avoid duplicate transaction errors.
