/**
 * Migrate AMOS Bounty Program to new mint
 *
 * Calls the update_mint instruction to atomically update:
 * - config.mint → new AMOS mint
 * - config.treasury → new treasury token account (PDA-owned)
 */

import { Connection, Keypair, PublicKey, Transaction, TransactionInstruction, SystemProgram } from "@solana/web3.js";
import { readFileSync } from "fs";
import { createHash } from "crypto";

// ── Config ────────────────────────────────────────────────────────────
const RPC_URL = "https://api.mainnet-beta.solana.com";
const BOUNTY_PROGRAM_ID = new PublicKey("4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq");
const NEW_MINT = new PublicKey("5g9vvce3YLsqZPBGAuKmGFfNKb5sp7v3Wiga5de8d5bQ");
const NEW_TREASURY = new PublicKey("9xDVHuW4kiUYH5NPDLFfKhpxLQ31N6bqMrvj4EJ57z2B");
const TOKEN_PROGRAM_ID = new PublicKey("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

// Load oracle keypair (founder wallet)
const keypairPath = process.env.HOME + "/amos-founder.json";
const keypairData = JSON.parse(readFileSync(keypairPath, "utf-8"));
const oracle = Keypair.fromSecretKey(new Uint8Array(keypairData));

console.log("Oracle/Payer:", oracle.publicKey.toBase58());
console.log("New Mint:", NEW_MINT.toBase58());
console.log("New Treasury:", NEW_TREASURY.toBase58());

// ── Derive PDA ──────────────────────────────────────────────────────
const BOUNTY_CONFIG_SEED = Buffer.from("bounty_config");
const [configPda] = PublicKey.findProgramAddressSync([BOUNTY_CONFIG_SEED], BOUNTY_PROGRAM_ID);
console.log("Config PDA:", configPda.toBase58());

// ── Build instruction data ──────────────────────────────────────────
// Anchor discriminator: sha256("global:update_mint")[0..8]
function anchorDiscriminator(name) {
    const hash = createHash("sha256").update(`global:${name}`).digest();
    return hash.slice(0, 8);
}

const discriminator = anchorDiscriminator("update_mint");
// No additional args — accounts carry the data
const data = Buffer.from(discriminator);

// ── Build transaction ───────────────────────────────────────────────
// Accounts for UpdateMint:
// 1. config (mut) — BountyConfig PDA
// 2. new_mint — the new AMOS token mint
// 3. new_treasury — new treasury token account
// 4. oracle_authority (signer) — the oracle
const ix = new TransactionInstruction({
    programId: BOUNTY_PROGRAM_ID,
    keys: [
        { pubkey: configPda, isSigner: false, isWritable: true },      // config
        { pubkey: NEW_MINT, isSigner: false, isWritable: false },       // new_mint
        { pubkey: NEW_TREASURY, isSigner: false, isWritable: false },   // new_treasury
        { pubkey: oracle.publicKey, isSigner: true, isWritable: false }, // oracle_authority
    ],
    data: data,
});

const conn = new Connection(RPC_URL, "confirmed");

async function main() {
    // Read current config to verify
    const configAccount = await conn.getAccountInfo(configPda);
    if (!configAccount) {
        console.error("BountyConfig not found!");
        process.exit(1);
    }

    // Skip 8-byte discriminator, read first 32 bytes = oracle_authority
    const oracleAuth = new PublicKey(configAccount.data.slice(8, 40));
    const currentMint = new PublicKey(configAccount.data.slice(40, 72));
    const currentTreasury = new PublicKey(configAccount.data.slice(72, 104));

    console.log("\nCurrent config:");
    console.log("  Oracle:", oracleAuth.toBase58());
    console.log("  Mint:", currentMint.toBase58());
    console.log("  Treasury:", currentTreasury.toBase58());

    console.log("\nSending update_mint transaction...");
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
    console.log("Mint migration complete!");

    // Verify
    const updatedAccount = await conn.getAccountInfo(configPda);
    const newMintOnChain = new PublicKey(updatedAccount.data.slice(40, 72));
    const newTreasuryOnChain = new PublicKey(updatedAccount.data.slice(72, 104));
    console.log("\nUpdated config:");
    console.log("  Mint:", newMintOnChain.toBase58());
    console.log("  Treasury:", newTreasuryOnChain.toBase58());
}

main().catch((err) => {
    console.error("Error:", err.message || err);
    if (err.logs) {
        console.error("Program logs:");
        err.logs.forEach(l => console.error("  ", l));
    }
    process.exit(1);
});
