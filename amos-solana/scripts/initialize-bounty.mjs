/**
 * Initialize AMOS Bounty Program on Devnet
 *
 * This script:
 * 1. Loads the oracle keypair (founder wallet)
 * 2. Derives the BountyConfig PDA
 * 3. Sends the initialize instruction with oracle_authority, mint, and treasury
 */

import { Connection, Keypair, PublicKey, Transaction, TransactionInstruction, SystemProgram } from "@solana/web3.js";
import { readFileSync } from "fs";
import { createHash } from "crypto";

// ── Config (env-overridable for mainnet) ────────────────────────────
const NETWORK = process.env.NETWORK || "devnet";
const RPC_URL = NETWORK === "mainnet"
    ? "https://api.mainnet-beta.solana.com"
    : "https://api.devnet.solana.com";
const BOUNTY_PROGRAM_ID = new PublicKey("4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq");
const MINT = new PublicKey(process.env.AMOS_MINT || "5g9vvce3YLsqZPBGAuKmGFfNKb5sp7v3Wiga5de8d5bQ");
const TREASURY = new PublicKey(process.env.AMOS_TREASURY || "9xDVHuW4kiUYH5NPDLFfKhpxLQ31N6bqMrvj4EJ57z2B");
const TOKEN_PROGRAM_ID = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

// Load oracle keypair (founder wallet)
const keypairPath = process.env.HOME + "/amos-founder.json";
const keypairData = JSON.parse(readFileSync(keypairPath, "utf-8"));
const oracle = Keypair.fromSecretKey(new Uint8Array(keypairData));

console.log("Oracle/Payer:", oracle.publicKey.toBase58());
console.log("Program:", BOUNTY_PROGRAM_ID.toBase58());
console.log("Mint:", MINT.toBase58());
console.log("Treasury:", TREASURY.toBase58());

// ── Derive PDA ──────────────────────────────────────────────────────
const BOUNTY_CONFIG_SEED = Buffer.from("bounty_config");
const [configPda, configBump] = PublicKey.findProgramAddressSync(
    [BOUNTY_CONFIG_SEED],
    BOUNTY_PROGRAM_ID
);
console.log("Config PDA:", configPda.toBase58(), "bump:", configBump);

// ── Build instruction data ──────────────────────────────────────────
// Anchor discriminator: sha256("global:initialize")[0..8]
function anchorDiscriminator(name) {
    const hash = createHash("sha256").update(`global:${name}`).digest();
    return hash.slice(0, 8);
}

const discriminator = anchorDiscriminator("initialize");
// Args: oracle_authority (Pubkey, 32 bytes)
const data = Buffer.alloc(8 + 32);
discriminator.copy(data, 0);
oracle.publicKey.toBuffer().copy(data, 8);

// ── Build transaction ───────────────────────────────────────────────
const ix = new TransactionInstruction({
    programId: BOUNTY_PROGRAM_ID,
    keys: [
        { pubkey: configPda, isSigner: false, isWritable: true },   // config (PDA, init)
        { pubkey: MINT, isSigner: false, isWritable: false },        // mint
        { pubkey: TREASURY, isSigner: false, isWritable: false },    // treasury
        { pubkey: oracle.publicKey, isSigner: true, isWritable: true }, // payer
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }, // system_program
    ],
    data: data,
});

const conn = new Connection(RPC_URL, "confirmed");

async function main() {
    // Check if already initialized
    const existingAccount = await conn.getAccountInfo(configPda);
    if (existingAccount) {
        console.log("\nBounty program already initialized!");
        console.log("Config account size:", existingAccount.data.length, "bytes");
        return;
    }

    console.log("\nSending initialize transaction...");
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
    console.log("Bounty program initialized successfully!");

    // Verify
    const account = await conn.getAccountInfo(configPda);
    console.log("Config account created:", account !== null);
    console.log("Config account size:", account?.data.length, "bytes");
}

main().catch((err) => {
    console.error("Error:", err.message || err);
    if (err.logs) {
        console.error("Program logs:");
        err.logs.forEach(l => console.error("  ", l));
    }
    process.exit(1);
});
