/**
 * Migrate AMOS Treasury Program to new mint
 *
 * Calls the update_mint instruction to atomically update:
 * - config.amos_mint → new AMOS mint
 * - config.treasury_amos_vault → new vault token account
 * - config.reserve_vault → new reserve token account
 */

import { Connection, Keypair, PublicKey, Transaction, TransactionInstruction } from "@solana/web3.js";
import { readFileSync } from "fs";
import { createHash } from "crypto";

const RPC_URL = "https://api.mainnet-beta.solana.com";
const TREASURY_PROGRAM_ID = new PublicKey("8ZMaZDAxDPsCnMGRkhwLmFhoG43WUJcGC8xqVKo2PN7s");
const NEW_MINT = new PublicKey("5g9vvce3YLsqZPBGAuKmGFfNKb5sp7v3Wiga5de8d5bQ");
const NEW_TREASURY_VAULT = new PublicKey("MKGji2LA9Fcn1sNhYZDHkXXuycvTDE48Uv9KEbjaufZ");
const NEW_RESERVE_VAULT = new PublicKey("Fa7MCS9hriDz82KqP1MY8ZXVKUzTthAJc4sQtGo9p1zn");

const keypairPath = process.env.HOME + "/amos-founder.json";
const keypairData = JSON.parse(readFileSync(keypairPath, "utf-8"));
const authority = Keypair.fromSecretKey(new Uint8Array(keypairData));

const [configPda] = PublicKey.findProgramAddressSync(
    [Buffer.from("treasury_config")],
    TREASURY_PROGRAM_ID
);

console.log("Authority:", authority.publicKey.toBase58());
console.log("Config PDA:", configPda.toBase58());
console.log("New Mint:", NEW_MINT.toBase58());
console.log("New Treasury Vault:", NEW_TREASURY_VAULT.toBase58());
console.log("New Reserve Vault:", NEW_RESERVE_VAULT.toBase58());

function anchorDiscriminator(name) {
    return createHash("sha256").update(`global:${name}`).digest().slice(0, 8);
}

// Accounts for UpdateMint:
// 1. authority (signer)
// 2. treasury_config (mut)
// 3. new_mint
// 4. new_treasury_vault
// 5. new_reserve_vault
const ix = new TransactionInstruction({
    programId: TREASURY_PROGRAM_ID,
    keys: [
        { pubkey: authority.publicKey, isSigner: true, isWritable: false },
        { pubkey: configPda, isSigner: false, isWritable: true },
        { pubkey: NEW_MINT, isSigner: false, isWritable: false },
        { pubkey: NEW_TREASURY_VAULT, isSigner: false, isWritable: false },
        { pubkey: NEW_RESERVE_VAULT, isSigner: false, isWritable: false },
    ],
    data: Buffer.from(anchorDiscriminator("update_mint")),
});

const conn = new Connection(RPC_URL, "confirmed");

async function main() {
    // Read current config
    const configAccount = await conn.getAccountInfo(configPda);
    const currentMint = new PublicKey(configAccount.data.slice(72, 104));
    console.log("\nCurrent mint in config:", currentMint.toBase58());

    console.log("Sending update_mint transaction...");
    const tx = new Transaction().add(ix);
    tx.feePayer = authority.publicKey;
    tx.recentBlockhash = (await conn.getLatestBlockhash()).blockhash;
    tx.sign(authority);

    const sig = await conn.sendRawTransaction(tx.serialize(), {
        skipPreflight: false,
        preflightCommitment: "confirmed",
    });
    console.log("Signature:", sig);
    await conn.confirmTransaction(sig, "confirmed");

    // Verify
    const updated = await conn.getAccountInfo(configPda);
    const newMint = new PublicKey(updated.data.slice(72, 104));
    const newVault = new PublicKey(updated.data.slice(104, 136));
    const newReserve = new PublicKey(updated.data.slice(136, 168));
    console.log("\nUpdated config:");
    console.log("  Mint:", newMint.toBase58());
    console.log("  Treasury Vault:", newVault.toBase58());
    console.log("  Reserve Vault:", newReserve.toBase58());
}

main().catch((err) => {
    console.error("Error:", err.message || err);
    if (err.logs) {
        console.error("Program logs:");
        err.logs.forEach(l => console.error("  ", l));
    }
    process.exit(1);
});
