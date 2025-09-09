import {
  Connection,
  PublicKey,
  Keypair,
  Transaction,
  TransactionInstruction,
  SystemProgram,
  sendAndConfirmTransaction,
} from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";
import * as crypto from "crypto";
import keccak256 from "keccak256";
import { executionAsyncId } from "async_hooks";

// Program configuration
const PROGRAM_ID = new PublicKey(
  "72bGikYM7J314fvAfBDvMGdqaewHaq7LpbJMNF5rJDb8"
);
const SHA256_IMAGE_ID =
  "75029efa53432a9030e5e76d58fb34dfa786cd0f6182ed0741d635ff5e4f0341";
const BONSOL_PROGRAM_ID = new PublicKey(
  "BoNsHRcyLLNdtnoDf8hiCNZpyehMC4FDMxs6NTxFi3ew"
);

interface EscrowData {
  seeds: Buffer;
  amountLamports: bigint;
  hash: string;
  isClaimed: boolean;
  receiver: PublicKey | null;
  initializer: PublicKey;
}

class HashLockedEscrowClient {
  private connection: Connection;
  private payer: Keypair;

  constructor(connection: Connection, payer: Keypair) {
    this.connection = connection;
    this.payer = payer;
  }

  // Get PDA for escrow account
  getEscrowAccountPDA(seed: Buffer): [PublicKey, number] {
    return PublicKey.findProgramAddressSync([seed], PROGRAM_ID);
  }

  // Get PDA for execution tracker
  getExecutionTrackerPDA(executionIdBuffer: Buffer): [PublicKey, number] {
    return PublicKey.findProgramAddressSync([executionIdBuffer], PROGRAM_ID);
  }

  // Generate SHA256 hash of input
  static generateSHA256Hash(input: string): string {
    return crypto.createHash("sha256").update(input, "utf8").digest("hex");
  }

  // Initialize escrow with hash-locked funds
  async initializeEscrow(
    seed: string,
    hashString: string,
    amountLamports: number
  ): Promise<{ escrowAccount: PublicKey; signature: string }> {
    const seedBuffer = Buffer.from(seed, "utf8");
    const [escrowAccount] = this.getEscrowAccountPDA(seedBuffer);

    // Validate hash is 64 hex characters
    if (!/^[a-fA-F0-9]{64}$/.test(hashString)) {
      throw new Error("Hash must be exactly 64 hexadecimal characters");
    }

    // Prepare instruction data
    const hashBuffer = Buffer.from(hashString, "utf8");
    const amountBuffer = Buffer.alloc(8);
    amountBuffer.writeBigUInt64LE(BigInt(amountLamports), 0);

    const data = Buffer.concat([
      Buffer.from([0]), // Instruction 0
      Buffer.from([seedBuffer.length]), // seed length
      seedBuffer, // seed
      Buffer.from([hashBuffer.length]), // hash length
      hashBuffer, // hash
      amountBuffer, // amount_lamports
    ]);

    const instruction = new TransactionInstruction({
      keys: [
        { pubkey: this.payer.publicKey, isSigner: true, isWritable: true },
        { pubkey: escrowAccount, isSigner: false, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      programId: PROGRAM_ID,
      data: data,
    });

    const transaction = new Transaction().add(instruction);
    const signature = await sendAndConfirmTransaction(
      this.connection,
      transaction,
      [this.payer]
    );

    console.log("Initialize escrow transaction signature:", signature);
    return { escrowAccount, signature };
  }

  // Claim escrow by providing the preimage
  async claimEscrow(
    seed: string,
    preimage: string,
    receiverPublicKey: PublicKey,
    executionId: string = "someExec1",
    tip: number = 1000,
    expiryOffset: number = 5000
  ): Promise<{
    requesterAccount: PublicKey;
    executionAccount: PublicKey;
    signature: string;
  }> {
    const seedBuffer = Buffer.from(seed, "utf8");
    const [escrowAccount] = this.getEscrowAccountPDA(seedBuffer);

    // Prepare execution ID and requester account
    const executionIdBuffer = Buffer.alloc(16);
    Buffer.from(executionId).copy(executionIdBuffer);
    const [requesterAccount, requesterBump] =
      this.getExecutionTrackerPDA(executionIdBuffer);

    // Bonsol program and related accounts
    const bonsolProgramId = BONSOL_PROGRAM_ID;
    const programIdAccount = PROGRAM_ID;

    const hash = keccak256(Buffer.from(SHA256_IMAGE_ID));
    const [imageIdAccount] = PublicKey.findProgramAddressSync(
      [Buffer.from("deployment"), hash],
      bonsolProgramId
    );

    const [executionAccount] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("execution"),
        this.payer.publicKey.toBuffer(),
        executionIdBuffer,
      ],
      bonsolProgramId
    );

    // Helper function to convert number to little-endian u64 buffer
    const numberToU64LE = (num: number): Buffer => {
      const buffer = Buffer.alloc(8);
      buffer.writeBigUInt64LE(BigInt(num), 0);
      return buffer;
    };

    // // Prepare preimage
    // const preimageBuffer = Buffer.from(preimage, "utf8");

    // // If you want a fixed preimage size (e.g., 32 bytes)
    // const FIXED_PREIMAGE_SIZE = 32;
    // const preimageBuffer = Buffer.alloc(FIXED_PREIMAGE_SIZE);
    // Buffer.from(preimage, "utf8").copy(preimageBuffer);
    // const preimageLength = Buffer.alloc(2);
    // preimageLength.writeUInt16LE(preimageBuffer.length, 0);
    // const preimageLength = Buffer.from([0, FIXED_PREIMAGE_SIZE]);

    // If you want a fixed preimage size (e.g., 32 bytes)
    const preimageBuffer = Buffer.from(preimage, "utf8");
    const preimageLength = Buffer.alloc(2);
    preimageLength.writeUInt16LE(preimageBuffer.length, 0);

    const data = Buffer.concat([
      Buffer.from([1]), // Instruction 1
      executionIdBuffer, // execution_id (16 bytes)
      Buffer.from([requesterBump]), // bump (1 byte)
      numberToU64LE(tip), // tip (8 bytes)
      numberToU64LE(expiryOffset), // expiry_offset (8 bytes)
      Buffer.from([seedBuffer.length]), // seed_len (1 byte)
      seedBuffer, // seed
      preimageLength, // preimage_len (2 bytes)
      preimageBuffer, // preimage
    ]);

    console.log("DEBUG: payer public key:", this.payer.publicKey.toString());
    console.log("DEBUG: receiver public key:", receiverPublicKey.toString());
    console.log("DEBUG: escrow account:", escrowAccount.toString());
    console.log("DEBUG: requester account:", requesterAccount.toString());
    console.log("DEBUG: execution account:", executionAccount.toString());
    console.log("DEBUG: bonsol program ID:", bonsolProgramId.toString());
    console.log("DEBUG: image ID account:", imageIdAccount.toString());
    console.log("DEBUG: program ID account:", programIdAccount.toString());
    console.log("DEBUG: preimage:", preimage);

    const instruction = new TransactionInstruction({
      keys: [
        { pubkey: this.payer.publicKey, isSigner: true, isWritable: true }, // payer
        { pubkey: receiverPublicKey, isSigner: false, isWritable: true }, // receiver
        { pubkey: escrowAccount, isSigner: false, isWritable: true }, // escrow_account
        { pubkey: requesterAccount, isSigner: false, isWritable: true }, // requester
        { pubkey: executionAccount, isSigner: false, isWritable: true }, // execution_account
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }, // system_program
        { pubkey: bonsolProgramId, isSigner: false, isWritable: false }, // bonsol_program
        { pubkey: imageIdAccount, isSigner: false, isWritable: false }, // image_id
        { pubkey: programIdAccount, isSigner: false, isWritable: false }, // program_id
      ],
      programId: PROGRAM_ID,
      data: data,
    });

    const transaction = new Transaction().add(instruction);
    const signature = await sendAndConfirmTransaction(
      this.connection,
      transaction,
      [this.payer]
    );

    console.log("Claim escrow transaction signature:", signature);
    return { requesterAccount, executionAccount, signature };
  }

  // Get escrow account data
  async getEscrowAccountData(seed: string): Promise<EscrowData> {
    const seedBuffer = Buffer.from(seed, "utf8");
    const [escrowAccount] = this.getEscrowAccountPDA(seedBuffer);
    const accountInfo = await this.connection.getAccountInfo(escrowAccount);

    if (!accountInfo) {
      throw new Error("Escrow account not found");
    }

    const data = accountInfo.data;

    // Parse escrow data according to the pack/unpack format
    const seeds = data.slice(0, 32);
    const amountLamports = data.readBigUInt64LE(32);
    const hashBytes = data.slice(40, 104);
    const hash = hashBytes.toString("utf8");
    const isClaimed = data[104] !== 0;

    // Parse Option<Pubkey>
    const hasReceiver = data[105] !== 0;
    const receiver = hasReceiver ? new PublicKey(data.slice(106, 138)) : null;

    const initializer = new PublicKey(data.slice(138, 170));

    return {
      seeds,
      amountLamports,
      hash,
      isClaimed,
      receiver,
      initializer,
    };
  }

  // Helper function to load keypair from file
  static loadKeypairFromFile(filePath: string): Keypair {
    const secretKeyString = fs.readFileSync(filePath, "utf8");
    const secretKey = Uint8Array.from(JSON.parse(secretKeyString));
    return Keypair.fromSecretKey(secretKey);
  }

  // Helper function to airdrop SOL for testing
  async airdropSol(lamports: number = 20000000000000000): Promise<void> {
    try {
      const airdropSignature = await this.connection.requestAirdrop(
        this.payer.publicKey,
        lamports
      );
      await this.connection.confirmTransaction(airdropSignature);
      console.log(
        `Airdropped ${
          lamports / 1000000000
        } SOL to ${this.payer.publicKey.toString()}`
      );
    } catch (error) {
      console.error("Airdrop failed:", error);
    }
  }
}

// Configuration
const RPC_URL = process.env.RPC_URL || "http://localhost:8899";
const KEYPAIR_PATH =
  process.env.KEYPAIR_PATH ||
  path.join(process.env.HOME || "", ".config/solana/id.json");

// Initialize connection and client
let connection: Connection;
let client: HashLockedEscrowClient;

// Demo functions

// Initialize escrow function
const initializeEscrow = async (
  seed: string,
  secret: string,
  amountSOL: number
): Promise<void> => {
  console.log("🔐 Initializing hash-locked escrow...");

  const hash = HashLockedEscrowClient.generateSHA256Hash(secret);
  const amountLamports = amountSOL * 1000000000; // Convert SOL to lamports

  console.log(`Secret: "${secret}"`);
  console.log(`Hash: ${hash}`);
  console.log(`Amount: ${amountSOL} SOL (${amountLamports} lamports)`);

  try {
    const result = await client.initializeEscrow(seed, hash, amountLamports);
    console.log("✅ Escrow initialized successfully!");
    console.log("Escrow Account:", result.escrowAccount.toString());
    console.log("Transaction Signature:", result.signature);

    // Display current escrow data
    const escrowData = await client.getEscrowAccountData(seed);
    console.log("\n📊 Escrow Data:");
    console.log(
      "Amount Locked:",
      escrowData.amountLamports.toString(),
      "lamports"
    );
    console.log("Hash:", escrowData.hash);
    console.log("Is Claimed:", escrowData.isClaimed);
    console.log("Initializer:", escrowData.initializer.toString());

    return;
  } catch (error) {
    console.error("❌ Initialize escrow failed:", error);
    throw error;
  }
};

// Claim escrow function
const claimEscrow = async (
  seed: string,
  secret: string,
  receiverKeypair: Keypair,
  executionId: string = "someExec1"
): Promise<void> => {
  console.log("🔓 Attempting to claim escrow...");

  const hash = HashLockedEscrowClient.generateSHA256Hash(secret);
  console.log(`Using secret: "${secret}"`);
  console.log(`Expected hash: ${hash}`);
  console.log(`Receiver: ${receiverKeypair.publicKey.toString()}`);

  try {
    // Check escrow data before claiming
    const escrowDataBefore = await client.getEscrowAccountData(seed);
    console.log("\n📊 Escrow Data (Before Claim):");
    console.log(
      "Amount Locked:",
      escrowDataBefore.amountLamports.toString(),
      "lamports"
    );
    console.log("Is Claimed:", escrowDataBefore.isClaimed);
    console.log("Hash matches:", escrowDataBefore.hash.trim() === hash);

    if (escrowDataBefore.isClaimed) {
      console.log("❌ Escrow already claimed!");
      return;
    }

    // Get receiver balance before
    const balanceBefore = await connection.getBalance(
      receiverKeypair.publicKey
    );
    console.log(`Receiver balance before: ${balanceBefore / 1000000000} SOL`);

    const result = await client.claimEscrow(
      seed,
      secret,
      receiverKeypair.publicKey,
      executionId
    );

    console.log("✅ Claim request submitted successfully!");
    console.log("Requester Account:", result.requesterAccount.toString());
    console.log("Execution Account:", result.executionAccount.toString());
    console.log("Transaction Signature:", result.signature);

    // Wait a bit for the callback to be processed
    console.log("\n⏳ Waiting for Bonsol callback processing...");
    await new Promise((resolve) => setTimeout(resolve, 10000)); // Wait 10 seconds

    // Check final state
    try {
      const escrowDataAfter = await client.getEscrowAccountData(seed);
      const balanceAfter = await connection.getBalance(
        receiverKeypair.publicKey
      );

      console.log("\n📊 Final Results:");
      console.log("Escrow Is Claimed:", escrowDataAfter.isClaimed);
      console.log("Receiver:", escrowDataAfter.receiver?.toString() || "None");
      console.log(`Receiver balance after: ${balanceAfter / 1000000000} SOL`);
      console.log(
        `Balance change: ${(balanceAfter - balanceBefore) / 1000000000} SOL`
      );

      if (escrowDataAfter.isClaimed) {
        console.log("🎉 Escrow successfully claimed!");
      } else {
        console.log("⚠️ Escrow claim may still be processing...");
      }
    } catch (dataError) {
      console.log("⚠️ Could not fetch final escrow data");
    }
  } catch (error) {
    console.error("❌ Claim escrow failed:", error);
    throw error;
  }
};

// Setup function
const setup = async (payer: Keypair): Promise<void> => {
  console.log("🔧 Setting up connection and client...");

  connection = new Connection(RPC_URL, "confirmed");

  // Try to load keypair from file, otherwise generate new one
  try {
    if (fs.existsSync(KEYPAIR_PATH)) {
      payer = HashLockedEscrowClient.loadKeypairFromFile(KEYPAIR_PATH);
      console.log("🔑 Loaded keypair from file:", KEYPAIR_PATH);
    } else {
      payer = Keypair.generate();
      console.log("🔑 Generated new keypair for testing");
    }
  } catch (error) {
    console.log("⚠️ Could not load keypair from file, generating new one");
    payer = Keypair.generate();
  }

  client = new HashLockedEscrowClient(connection, payer);

  console.log("💼 Payer public key:", payer.publicKey.toString());
  await client.airdropSol();

  // Check balance and airdrop if needed (for local testing)
  const balance = await connection.getBalance(payer.publicKey);
  console.log("💰 Current balance:", balance / 1000000000, "SOL");

  // if (balance < 1000000000 && RPC_URL.includes("localhost")) {
  //   console.log("💸 Requesting airdrop...");

  // }
};

// Main execution function
const main = async (): Promise<void> => {
  console.log("🌟 Starting Hash-Locked Escrow Demo\n");

  try {
    let payer: Keypair;
    payer = HashLockedEscrowClient.loadKeypairFromFile(KEYPAIR_PATH);
    console.log("🔑 Loaded keypair from file:", KEYPAIR_PATH);
    // Setup connection and client
    await setup(payer);

    console.log("\n" + "=".repeat(60));

    // Demo parameters
    const escrowSeed = "my-escrow-seed-1237";
    const secret = "my-secret-password-3";
    const executionId = "someExec2";
    const amountSOL = 0.1; // 0.1 SOL

    // Initialize escrow
    await initializeEscrow(escrowSeed, secret, amountSOL);

    console.log("\n" + "=".repeat(60));

    // Wait a moment
    await new Promise((resolve) => setTimeout(resolve, 2000));

    // // Claim escrow
    const secretUrl = "http://localhost:3000"; // URL of the data server
    await claimEscrow(escrowSeed, secretUrl, payer, executionId);

    console.log("\n" + "=".repeat(60));
    console.log("🎉 Hash-locked escrow demo completed!");

    console.log("\n💡 How it works:");
    console.log("1. Escrow is initialized with funds locked by a SHA256 hash");
    console.log("2. To claim, the claimant provides the preimage (secret)");
    console.log("3. Bonsol computes the SHA256 hash of the preimage");
    console.log(
      "4. If the computed hash matches the stored hash, funds are released"
    );
    console.log(
      "5. This enables trustless atomic swaps and conditional payments"
    );
  } catch (error) {
    console.error("\n💥 Demo failed:", error);
    throw error;
  }
};

// Execute main function
main().catch((err) => {
  console.error(err);
  process.exit(1);
});

// Export for use as module
export { HashLockedEscrowClient };
