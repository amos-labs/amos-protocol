/**
 * AMOS Treasury Rebalance Script
 *
 * Moves tokens from the Treasury program vault to the correct locations:
 *   1. 90M AMOS → Bounty program treasury (emission pool for bounty payouts)
 *   2.  5M AMOS → Treasury reserve vault (DAO-locked emergency reserve)
 *
 * This fixes the initial deployment where 95M landed in the Treasury
 * program's fee-distribution vault instead of the Bounty program's
 * emission pool. Per the docs (whitepaper, AGENT_CONTEXT.md, token_economy_math.md):
 *   - 95% of supply = Bounty Treasury (contributor rewards via daily emission)
 *   - 5% of supply  = Emergency Reserve (DAO-locked, governance vote to deploy)
 *
 * Prerequisites:
 *   - Treasury program upgraded with authority_withdraw instruction
 *   - Oracle keypair at ~/amos-founder.json (is Treasury authority)
 *
 * Usage:
 *   NETWORK=mainnet node scripts/rebalance-treasury.mjs
 */

import { Connection, Keypair, PublicKey, Transaction, TransactionInstruction } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID, getAccount } from "@solana/spl-token";
import { readFileSync } from "fs";
import { createHash } from "crypto";

// ── Config ─────────────────────────────────────────────────────────
const NETWORK = process.env.NETWORK || "devnet";
const RPC_URL = NETWORK === "mainnet"
    ? "https://api.mainnet-beta.solana.com"
    : "https://api.devnet.solana.com";

const TREASURY_PROGRAM_ID = new PublicKey("8ZMaZDAxDPsCnMGRkhwLmFhoG43WUJcGC8xqVKo2PN7s");
const AMOS_MINT = new PublicKey(
    NETWORK === "mainnet"
        ? "5g9vvce3YLsqZPBGAuKmGFfNKb5sp7v3Wiga5de8d5bQ"
        : process.env.AMOS_MINT || "5g9vvce3YLsqZPBGAuKmGFfNKb5sp7v3Wiga5de8d5bQ"
);

// Bounty program's treasury token account (owned by BountyConfig PDA)
const BOUNTY_TREASURY = new PublicKey("9xDVHuW4kiUYH5NPDLFfKhpxLQ31N6bqMrvj4EJ57z2B");

const DECIMALS = 9;
const BOUNTY_AMOUNT = BigInt(90_000_000) * BigInt(10 ** DECIMALS);   // 90M AMOS
const RESERVE_AMOUNT = BigInt(5_000_000) * BigInt(10 ** DECIMALS);   // 5M AMOS

// Load authority keypair
const keypairPath = process.env.HOME + "/amos-founder.json";
const keypairData = JSON.parse(readFileSync(keypairPath, "utf-8"));
const authority = Keypair.fromSecretKey(new Uint8Array(keypairData));

const conn = new Connection(RPC_URL, "confirmed");

// ── Anchor discriminator helper ────────────────────────────────────
function anchorDiscriminator(name) {
    return createHash("sha256").update(`global:${name}`).digest().slice(0, 8);
}

// ── Derive PDAs ────────────────────────────────────────────────────
const [treasuryConfig] = PublicKey.findProgramAddressSync(
    [Buffer.from("treasury_config")], TREASURY_PROGRAM_ID
);
const [treasuryAmosVault] = PublicKey.findProgramAddressSync(
    [Buffer.from("treasury_amos")], TREASURY_PROGRAM_ID
);
const [reserveVault] = PublicKey.findProgramAddressSync(
    [Buffer.from("reserve_vault")], TREASURY_PROGRAM_ID
);

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
    console.log(`  ✓ ${name} confirmed!`);
    return sig;
}

function buildAuthorityWithdraw(amount, destination) {
    // authority_withdraw(amount: u64)
    const disc = anchorDiscriminator("authority_withdraw");
    const data = Buffer.alloc(8 + 8);
    disc.copy(data, 0);
    // Write u64 little-endian
    data.writeBigUInt64LE(amount, 8);

    return new TransactionInstruction({
        programId: TREASURY_PROGRAM_ID,
        keys: [
            // authority: Signer
            { pubkey: authority.publicKey, isSigner: true, isWritable: false },
            // treasury_config: PDA (seeds check + has_one authority + has_one amos_mint)
            { pubkey: treasuryConfig, isSigner: false, isWritable: false },
            // amos_mint
            { pubkey: AMOS_MINT, isSigner: false, isWritable: false },
            // treasury_amos_vault: PDA token account (mut, source)
            { pubkey: treasuryAmosVault, isSigner: false, isWritable: true },
            // destination: token account (mut, target)
            { pubkey: destination, isSigner: false, isWritable: true },
            // token_program
            { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        ],
        data,
    });
}

async function main() {
    console.log("=".repeat(60));
    console.log("AMOS Treasury Rebalance");
    console.log("=".repeat(60));
    console.log("Network:", NETWORK);
    console.log("Authority:", authority.publicKey.toBase58());
    console.log("Treasury Program:", TREASURY_PROGRAM_ID.toBase58());
    console.log("AMOS Mint:", AMOS_MINT.toBase58());
    console.log("\nPDAs:");
    console.log("  Treasury Config:", treasuryConfig.toBase58());
    console.log("  Treasury AMOS Vault:", treasuryAmosVault.toBase58());
    console.log("  Reserve Vault:", reserveVault.toBase58());
    console.log("\nDestinations:");
    console.log("  Bounty Treasury:", BOUNTY_TREASURY.toBase58());
    console.log("  Reserve Vault:", reserveVault.toBase58());

    // ── Pre-flight: check balances ─────────────────────────────────
    console.log("\n--- Pre-flight Balances ---");

    const vaultInfo = await getAccount(conn, treasuryAmosVault);
    const vaultBalance = vaultInfo.amount;
    console.log("  Treasury Vault:", (Number(vaultBalance) / 10 ** DECIMALS).toLocaleString(), "AMOS");

    const bountyInfo = await getAccount(conn, BOUNTY_TREASURY);
    console.log("  Bounty Treasury:", (Number(bountyInfo.amount) / 10 ** DECIMALS).toLocaleString(), "AMOS");

    const reserveInfo = await getAccount(conn, reserveVault);
    console.log("  Reserve Vault:", (Number(reserveInfo.amount) / 10 ** DECIMALS).toLocaleString(), "AMOS");

    const totalToMove = BOUNTY_AMOUNT + RESERVE_AMOUNT;
    if (vaultBalance < totalToMove) {
        console.error(`\nERROR: Treasury vault has ${vaultBalance} but need ${totalToMove}`);
        process.exit(1);
    }

    console.log("\n--- Planned Transfers ---");
    console.log(`  90,000,000 AMOS → Bounty Treasury (${BOUNTY_TREASURY.toBase58().slice(0, 8)}...)`);
    console.log(`   5,000,000 AMOS → Reserve Vault   (${reserveVault.toBase58().slice(0, 8)}...)`);
    console.log(`  Remaining in vault: ${((Number(vaultBalance) - Number(totalToMove)) / 10 ** DECIMALS).toLocaleString()} AMOS`);

    if (process.env.DRY_RUN === "true") {
        console.log("\n[DRY RUN] Exiting without sending transactions.");
        return;
    }

    // ── Step 1: Move 90M to Bounty Treasury ────────────────────────
    const ix1 = buildAuthorityWithdraw(BOUNTY_AMOUNT, BOUNTY_TREASURY);
    await sendTx("Step 1: 90M AMOS → Bounty Treasury (emission pool)", ix1);

    // ── Step 2: Move 5M to Reserve Vault ───────────────────────────
    const ix2 = buildAuthorityWithdraw(RESERVE_AMOUNT, reserveVault);
    await sendTx("Step 2: 5M AMOS → Reserve Vault (emergency reserve)", ix2);

    // ── Verify ─────────────────────────────────────────────────────
    console.log("\n--- Post-Transfer Balances ---");

    const vaultAfter = await getAccount(conn, treasuryAmosVault);
    console.log("  Treasury Vault:", (Number(vaultAfter.amount) / 10 ** DECIMALS).toLocaleString(), "AMOS");

    const bountyAfter = await getAccount(conn, BOUNTY_TREASURY);
    console.log("  Bounty Treasury:", (Number(bountyAfter.amount) / 10 ** DECIMALS).toLocaleString(), "AMOS");

    const reserveAfter = await getAccount(conn, reserveVault);
    console.log("  Reserve Vault:", (Number(reserveAfter.amount) / 10 ** DECIMALS).toLocaleString(), "AMOS");

    console.log("\n" + "=".repeat(60));
    console.log("Treasury rebalance complete!");
    console.log("  Bounty emission pool funded with 90M AMOS");
    console.log("  Emergency reserve funded with 5M AMOS");
    console.log("=".repeat(60));
}

main().catch((err) => {
    console.error("\nError:", err.message || err);
    if (err.logs) {
        console.error("Program logs:");
        err.logs.forEach(l => console.error("  ", l));
    }
    process.exit(1);
});
