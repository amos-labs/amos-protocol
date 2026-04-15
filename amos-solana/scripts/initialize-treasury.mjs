/**
 * Initialize AMOS Treasury Program on Devnet (3-step process)
 *
 * Step 1: initialize — creates treasury_config + holder_pool
 * Step 2: initialize_vaults — creates treasury_amos_vault
 * Step 3: initialize_reserve — creates reserve_vault
 *
 * Prerequisites:
 * - Treasury program deployed at TREASURY_PROGRAM_ID
 * - AMOS mint exists on devnet
 * - Oracle keypair at ~/amos-founder.json
 */

import { Connection, Keypair, PublicKey, Transaction, TransactionInstruction, SystemProgram } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";
import { readFileSync } from "fs";
import { createHash } from "crypto";

// ── Config (env-overridable for mainnet) ────────────────────────────
const NETWORK = process.env.NETWORK || "devnet";
const RPC_URL = NETWORK === "mainnet"
    ? "https://api.mainnet-beta.solana.com"
    : "https://api.devnet.solana.com";
const TREASURY_PROGRAM_ID = new PublicKey("8ZMaZDAxDPsCnMGRkhwLmFhoG43WUJcGC8xqVKo2PN7s");
const AMOS_MINT = new PublicKey(process.env.AMOS_MINT || "5g9vvce3YLsqZPBGAuKmGFfNKb5sp7v3Wiga5de8d5bQ");

// Load oracle keypair (authority)
const keypairPath = process.env.HOME + "/amos-founder.json";
const keypairData = JSON.parse(readFileSync(keypairPath, "utf-8"));
const authority = Keypair.fromSecretKey(new Uint8Array(keypairData));

const conn = new Connection(RPC_URL, "confirmed");

console.log("Authority:", authority.publicKey.toBase58());
console.log("Treasury Program:", TREASURY_PROGRAM_ID.toBase58());
console.log("AMOS Mint:", AMOS_MINT.toBase58());

// ── Anchor discriminator helper ─────────────────────────────────────
function anchorDiscriminator(name) {
    return createHash("sha256").update(`global:${name}`).digest().slice(0, 8);
}

// ── Derive PDAs ─────────────────────────────────────────────────────
const [treasuryConfig, treasuryConfigBump] = PublicKey.findProgramAddressSync(
    [Buffer.from("treasury_config")], TREASURY_PROGRAM_ID
);
const [holderPool, holderPoolBump] = PublicKey.findProgramAddressSync(
    [Buffer.from("holder_pool")], TREASURY_PROGRAM_ID
);
const [treasuryAmosVault, treasuryAmosVaultBump] = PublicKey.findProgramAddressSync(
    [Buffer.from("treasury_amos")], TREASURY_PROGRAM_ID
);
const [reserveVault, reserveVaultBump] = PublicKey.findProgramAddressSync(
    [Buffer.from("reserve_vault")], TREASURY_PROGRAM_ID
);

console.log("\nPDAs:");
console.log("  Treasury Config:", treasuryConfig.toBase58());
console.log("  Holder Pool:", holderPool.toBase58());
console.log("  Treasury AMOS Vault:", treasuryAmosVault.toBase58());
console.log("  Reserve Vault:", reserveVault.toBase58());

// Use the Labs wallet = authority for devnet
const LABS_WALLET = authority.publicKey;

async function sendTx(name, ix) {
    console.log(`\nSending ${name}...`);
    const tx = new Transaction().add(ix);
    tx.feePayer = authority.publicKey;
    tx.recentBlockhash = (await conn.getLatestBlockhash()).blockhash;
    tx.sign(authority);

    const sig = await conn.sendRawTransaction(tx.serialize(), {
        skipPreflight: false,
        preflightCommitment: "confirmed",
    });
    console.log("  Signature:", sig);
    await conn.confirmTransaction(sig, "confirmed");
    console.log(`  ${name} confirmed!`);
    return sig;
}

async function main() {
    // ── Step 1: Initialize (config + holder_pool) ─────────────────────
    const configExists = await conn.getAccountInfo(treasuryConfig);
    if (configExists) {
        console.log("\n✓ Treasury config already exists (" + configExists.data.length + " bytes)");
    } else {
        // initialize(labs_wallet: Pubkey)
        const data = Buffer.alloc(8 + 32);
        anchorDiscriminator("initialize").copy(data, 0);
        LABS_WALLET.toBuffer().copy(data, 8);

        const ix = new TransactionInstruction({
            programId: TREASURY_PROGRAM_ID,
            keys: [
                { pubkey: authority.publicKey, isSigner: true, isWritable: true },
                { pubkey: treasuryConfig, isSigner: false, isWritable: true },
                { pubkey: holderPool, isSigner: false, isWritable: true },
                { pubkey: AMOS_MINT, isSigner: false, isWritable: false },
                { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
            ],
            data,
        });

        await sendTx("Step 1: Initialize (config + holder_pool)", ix);
    }

    // ── Step 2: Initialize Vaults (treasury AMOS vault) ───────────────
    const vaultExists = await conn.getAccountInfo(treasuryAmosVault);
    if (vaultExists) {
        console.log("\n✓ Treasury AMOS vault already exists (" + vaultExists.data.length + " bytes)");
    } else {
        const data = anchorDiscriminator("initialize_vaults");

        const SYSVAR_RENT = new PublicKey("SysvarRent111111111111111111111111111111111");

        const ix = new TransactionInstruction({
            programId: TREASURY_PROGRAM_ID,
            keys: [
                { pubkey: authority.publicKey, isSigner: true, isWritable: true },
                { pubkey: treasuryConfig, isSigner: false, isWritable: true },
                { pubkey: AMOS_MINT, isSigner: false, isWritable: false },
                { pubkey: treasuryAmosVault, isSigner: false, isWritable: true },
                { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
                { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
                { pubkey: SYSVAR_RENT, isSigner: false, isWritable: false },
            ],
            data,
        });

        await sendTx("Step 2: Initialize Vaults (AMOS vault)", ix);
    }

    // ── Step 3: Initialize Reserve (reserve vault) ─────────────────────
    const reserveExists = await conn.getAccountInfo(reserveVault);
    if (reserveExists) {
        console.log("\n✓ Reserve vault already exists (" + reserveExists.data.length + " bytes)");
    } else {
        const data = anchorDiscriminator("initialize_reserve");

        const SYSVAR_RENT = new PublicKey("SysvarRent111111111111111111111111111111111");

        const ix = new TransactionInstruction({
            programId: TREASURY_PROGRAM_ID,
            keys: [
                { pubkey: authority.publicKey, isSigner: true, isWritable: true },
                { pubkey: treasuryConfig, isSigner: false, isWritable: true },
                { pubkey: AMOS_MINT, isSigner: false, isWritable: false },
                { pubkey: reserveVault, isSigner: false, isWritable: true },
                { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
                { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
                { pubkey: SYSVAR_RENT, isSigner: false, isWritable: false },
            ],
            data,
        });

        await sendTx("Step 3: Initialize Reserve (reserve vault)", ix);
    }

    // ── Verify ─────────────────────────────────────────────────────────
    console.log("\n" + "=".repeat(60));
    console.log("Verification:");
    for (const [name, pda] of [
        ["Treasury Config", treasuryConfig],
        ["Holder Pool", holderPool],
        ["Treasury AMOS Vault", treasuryAmosVault],
        ["Reserve Vault", reserveVault],
    ]) {
        const info = await conn.getAccountInfo(pda);
        console.log(`  ${name}: ${info ? info.data.length + " bytes" : "MISSING!"}`);
    }
    console.log("=".repeat(60));
    console.log("Treasury initialization complete!");
}

main().catch((err) => {
    console.error("\nError:", err.message || err);
    if (err.logs) {
        console.error("Program logs:");
        err.logs.forEach(l => console.error("  ", l));
    }
    process.exit(1);
});
