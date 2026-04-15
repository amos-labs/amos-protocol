/**
 * Set Fee Recipients on AMOS Bounty Program (Devnet)
 *
 * Calls set_fee_recipients to configure:
 * - holder_pool: Treasury AMOS vault (receives 50% of commercial bounty fees)
 * - labs_wallet: Oracle's ATA (receives 10% of commercial bounty fees)
 */

import { Connection, Keypair, PublicKey, Transaction, TransactionInstruction } from "@solana/web3.js";
import { readFileSync } from "fs";
import { createHash } from "crypto";

// ── Config ──────────────────────────────────────────────────────────
const RPC_URL = "https://api.devnet.solana.com";
const BOUNTY_PROGRAM_ID = new PublicKey("4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq");
const MINT = new PublicKey("5g9vvce3YLsqZPBGAuKmGFfNKb5sp7v3Wiga5de8d5bQ");

// Fee recipient token accounts (both must be AMOS token accounts for the new mint)
// TODO: Create new holder_pool and labs_wallet token accounts for the new mint
const HOLDER_POOL = new PublicKey("6fh9KrCT7Jv9WFMhyYQSMXwr8wu59bS2fUuv7nmJuhZX"); // needs new-mint account
const LABS_WALLET = new PublicKey("Es4SCAKj6ncLTkrCkF3RnGQjvtGJFBFrfpLTQdhtHjET"); // needs new-mint account

// Load oracle keypair
const keypairPath = process.env.HOME + "/amos-founder.json";
const keypairData = JSON.parse(readFileSync(keypairPath, "utf-8"));
const oracle = Keypair.fromSecretKey(new Uint8Array(keypairData));

console.log("Oracle:", oracle.publicKey.toBase58());
console.log("Holder Pool:", HOLDER_POOL.toBase58());
console.log("Labs Wallet:", LABS_WALLET.toBase58());

// ── Derive Config PDA ───────────────────────────────────────────────
const [configPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("bounty_config")],
    BOUNTY_PROGRAM_ID
);
console.log("Config PDA:", configPda.toBase58());

// ── Build instruction ───────────────────────────────────────────────
// Anchor discriminator: sha256("global:set_fee_recipients")[0..8]
const discriminator = createHash("sha256")
    .update("global:set_fee_recipients")
    .digest()
    .slice(0, 8);

// No args — accounts only
const data = Buffer.from(discriminator);

// Accounts: config (mut), mint, holder_pool, labs_wallet, oracle_authority (signer)
const ix = new TransactionInstruction({
    programId: BOUNTY_PROGRAM_ID,
    keys: [
        { pubkey: configPda, isSigner: false, isWritable: true },       // config
        { pubkey: MINT, isSigner: false, isWritable: false },            // mint
        { pubkey: HOLDER_POOL, isSigner: false, isWritable: false },     // holder_pool
        { pubkey: LABS_WALLET, isSigner: false, isWritable: false },     // labs_wallet
        { pubkey: oracle.publicKey, isSigner: true, isWritable: false }, // oracle_authority
    ],
    data: data,
});

const conn = new Connection(RPC_URL, "confirmed");

async function main() {
    // Verify config exists
    const configAccount = await conn.getAccountInfo(configPda);
    if (!configAccount) {
        console.error("Config PDA not found — initialize bounty program first");
        process.exit(1);
    }

    // Read current holder_pool and labs_wallet from config
    // holder_pool offset: 8(disc) + 32 + 32 + 32 + 8 + 1 + 8 + 8 + 8 + 8 + 2 + 1 = 148
    // labs_wallet offset: 148 + 32 = 180
    const configData = configAccount.data;
    const currentHolderPool = new PublicKey(configData.slice(148, 180));
    const currentLabsWallet = new PublicKey(configData.slice(180, 212));
    console.log("\nCurrent holder_pool:", currentHolderPool.toBase58());
    console.log("Current labs_wallet:", currentLabsWallet.toBase58());

    console.log("\nSending set_fee_recipients transaction...");
    const tx = new Transaction().add(ix);
    tx.feePayer = oracle.publicKey;
    tx.recentBlockhash = (await conn.getLatestBlockhash()).blockhash;
    tx.sign(oracle);

    const sig = await conn.sendRawTransaction(tx.serialize(), {
        skipPreflight: false,
        preflightCommitment: "confirmed",
    });
    console.log("Signature:", sig);

    await conn.confirmTransaction(sig, "confirmed");
    console.log("set_fee_recipients succeeded!");

    // Verify update
    const updatedAccount = await conn.getAccountInfo(configPda);
    const updatedData = updatedAccount.data;
    const newHolderPool = new PublicKey(updatedData.slice(148, 180));
    const newLabsWallet = new PublicKey(updatedData.slice(180, 212));
    console.log("\nUpdated holder_pool:", newHolderPool.toBase58());
    console.log("Updated labs_wallet:", newLabsWallet.toBase58());

    // Verify they match expected values
    if (newHolderPool.equals(HOLDER_POOL) && newLabsWallet.equals(LABS_WALLET)) {
        console.log("\n✓ Fee recipients configured correctly!");
    } else {
        console.error("\n✗ Fee recipients don't match expected values!");
        process.exit(1);
    }
}

main().catch((err) => {
    console.error("Error:", err.message || err);
    if (err.logs) {
        console.error("Program logs:");
        err.logs.forEach(l => console.error("  ", l));
    }
    process.exit(1);
});
