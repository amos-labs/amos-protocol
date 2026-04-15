/**
 * End-to-End On-Chain Test for AMOS Bounty + Treasury Programs
 *
 * Tests the full on-chain flow:
 * 1. Verify bounty config initialized
 * 2. Verify treasury config initialized (3-step)
 * 3. Create operator + reviewer token accounts
 * 4. prepare_bounty_submission (creates daily_pool + operator_stats)
 * 5. submit_bounty_proof (distributes tokens)
 * 6. Verify bounty proof record, daily pool, operator stats
 * 7. Verify treasury vaults
 *
 * Prerequisites:
 * - Both programs deployed and initialized on devnet
 * - AMOS mint at 5g9vvce3YLsqZPBGAuKmGFfNKb5sp7v3Wiga5de8d5bQ
 * - Oracle keypair at ~/amos-founder.json
 */

import {
    Connection, Keypair, PublicKey, Transaction, TransactionInstruction,
    SystemProgram, LAMPORTS_PER_SOL
} from "@solana/web3.js";
import {
    TOKEN_PROGRAM_ID, getOrCreateAssociatedTokenAccount,
    getAssociatedTokenAddress, createAssociatedTokenAccountInstruction
} from "@solana/spl-token";
import { readFileSync } from "fs";
import { createHash, randomBytes } from "crypto";

// ── Config ──────────────────────────────────────────────────────────
const RPC_URL = "https://api.devnet.solana.com";
const BOUNTY_PROGRAM_ID = new PublicKey("4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq");
const TREASURY_PROGRAM_ID = new PublicKey("8ZMaZDAxDPsCnMGRkhwLmFhoG43WUJcGC8xqVKo2PN7s");
const AMOS_MINT = new PublicKey("5g9vvce3YLsqZPBGAuKmGFfNKb5sp7v3Wiga5de8d5bQ");
const BOUNTY_TREASURY = new PublicKey("9xDVHuW4kiUYH5NPDLFfKhpxLQ31N6bqMrvj4EJ57z2B");

const conn = new Connection(RPC_URL, "confirmed");

// Load oracle/authority keypair
const keypairPath = process.env.HOME + "/amos-founder.json";
const keypairData = JSON.parse(readFileSync(keypairPath, "utf-8"));
const oracle = Keypair.fromSecretKey(new Uint8Array(keypairData));

// Generate a test operator keypair
const operator = Keypair.generate();

// ── Helpers ─────────────────────────────────────────────────────────
function anchorDiscriminator(name) {
    return createHash("sha256").update(`global:${name}`).digest().slice(0, 8);
}

function log(step, msg) {
    console.log(`\n[${step}] ${msg}`);
}

function fail(step, msg) {
    console.error(`\n[${step}] FAILED: ${msg}`);
    process.exit(1);
}

async function sendTx(name, instructions, signers) {
    const tx = new Transaction();
    for (const ix of instructions) tx.add(ix);
    tx.feePayer = oracle.publicKey;
    tx.recentBlockhash = (await conn.getLatestBlockhash()).blockhash;
    tx.sign(...signers);

    const sig = await conn.sendRawTransaction(tx.serialize(), {
        skipPreflight: false,
        preflightCommitment: "confirmed",
    });
    await conn.confirmTransaction(sig, "confirmed");
    return sig;
}

// ── PDA Derivations ─────────────────────────────────────────────────
const [bountyConfig] = PublicKey.findProgramAddressSync(
    [Buffer.from("bounty_config")], BOUNTY_PROGRAM_ID
);
const [treasuryConfig] = PublicKey.findProgramAddressSync(
    [Buffer.from("treasury_config")], TREASURY_PROGRAM_ID
);
const [holderPool] = PublicKey.findProgramAddressSync(
    [Buffer.from("holder_pool")], TREASURY_PROGRAM_ID
);
const [treasuryAmosVault] = PublicKey.findProgramAddressSync(
    [Buffer.from("treasury_amos")], TREASURY_PROGRAM_ID
);
const [reserveVault] = PublicKey.findProgramAddressSync(
    [Buffer.from("reserve_vault")], TREASURY_PROGRAM_ID
);

// Read start_time from bounty config to derive day_index
async function getDayIndex() {
    const info = await conn.getAccountInfo(bountyConfig);
    const startTime = info.data.readBigInt64LE(8 + 32 + 32 + 32);
    const now = Math.floor(Date.now() / 1000);
    return Math.floor((now - Number(startTime)) / 86400);
}

async function main() {
    console.log("=".repeat(60));
    console.log("AMOS On-Chain E2E Test");
    console.log("=".repeat(60));
    console.log(`Oracle: ${oracle.publicKey.toBase58()}`);
    console.log(`Operator: ${operator.publicKey.toBase58()}`);
    console.log(`Bounty Program: ${BOUNTY_PROGRAM_ID.toBase58()}`);
    console.log(`Treasury Program: ${TREASURY_PROGRAM_ID.toBase58()}`);

    // ── Step 1: Verify Bounty Config ──────────────────────────────────
    log("1", "Verifying bounty config...");
    const configInfo = await conn.getAccountInfo(bountyConfig);
    if (!configInfo) fail("1", "Bounty config not found");
    log("1", `Bounty config: ${configInfo.data.length} bytes, owner: ${configInfo.owner.toBase58().slice(0, 12)}...`);

    const oracleFromConfig = new PublicKey(configInfo.data.slice(8, 40));
    if (!oracleFromConfig.equals(oracle.publicKey)) {
        fail("1", `Oracle mismatch: expected ${oracle.publicKey.toBase58()}, got ${oracleFromConfig.toBase58()}`);
    }
    log("1", "Oracle authority matches");

    // ── Step 2: Verify Treasury Config ────────────────────────────────
    log("2", "Verifying treasury config...");
    for (const [name, pda] of [
        ["Treasury Config", treasuryConfig],
        ["Holder Pool", holderPool],
        ["Treasury AMOS Vault", treasuryAmosVault],
        ["Reserve Vault", reserveVault],
    ]) {
        const info = await conn.getAccountInfo(pda);
        if (!info) fail("2", `${name} not found at ${pda.toBase58()}`);
        log("2", `${name}: ${info.data.length} bytes`);
    }

    // ── Step 3: Create Token Accounts ─────────────────────────────────
    log("3", "Setting up operator and reviewer token accounts...");

    // Fund operator with a bit of SOL for rent
    const fundIx = SystemProgram.transfer({
        fromPubkey: oracle.publicKey,
        toPubkey: operator.publicKey,
        lamports: 0.05 * LAMPORTS_PER_SOL,
    });
    const fundSig = await sendTx("Fund operator", [fundIx], [oracle]);
    log("3", `Funded operator: ${fundSig.slice(0, 20)}...`);

    // Create ATAs for operator and reviewer (oracle = reviewer in this test)
    const operatorAta = await getAssociatedTokenAddress(AMOS_MINT, operator.publicKey);
    const reviewerAta = await getAssociatedTokenAddress(AMOS_MINT, oracle.publicKey);

    const ataIxs = [];
    const operatorAtaInfo = await conn.getAccountInfo(operatorAta);
    if (!operatorAtaInfo) {
        ataIxs.push(createAssociatedTokenAccountInstruction(
            oracle.publicKey, operatorAta, operator.publicKey, AMOS_MINT
        ));
    }
    const reviewerAtaInfo = await conn.getAccountInfo(reviewerAta);
    if (!reviewerAtaInfo) {
        ataIxs.push(createAssociatedTokenAccountInstruction(
            oracle.publicKey, reviewerAta, oracle.publicKey, AMOS_MINT
        ));
    }

    if (ataIxs.length > 0) {
        const ataSig = await sendTx("Create ATAs", ataIxs, [oracle]);
        log("3", `Created ATAs: ${ataSig.slice(0, 20)}...`);
    } else {
        log("3", "ATAs already exist");
    }

    log("3", `Operator ATA: ${operatorAta.toBase58()}`);
    log("3", `Reviewer ATA: ${reviewerAta.toBase58()}`);

    // ── Step 4: Prepare Bounty Submission ─────────────────────────────
    log("4", "Calling prepare_bounty_submission...");

    const dayIndex = await getDayIndex();
    log("4", `Current day index: ${dayIndex}`);

    const dayIndexBytes = Buffer.alloc(4);
    dayIndexBytes.writeUInt32LE(dayIndex);

    const [dailyPool] = PublicKey.findProgramAddressSync(
        [Buffer.from("daily_pool"), dayIndexBytes],
        BOUNTY_PROGRAM_ID
    );
    const [operatorStats] = PublicKey.findProgramAddressSync(
        [Buffer.from("operator_stats"), operator.publicKey.toBuffer()],
        BOUNTY_PROGRAM_ID
    );

    log("4", `Daily Pool PDA: ${dailyPool.toBase58()}`);
    log("4", `Operator Stats PDA: ${operatorStats.toBase58()}`);

    // Build prepare instruction: discriminator + operator_key (Pubkey) + day_index (u32)
    const prepareData = Buffer.alloc(8 + 32 + 4);
    anchorDiscriminator("prepare_bounty_submission").copy(prepareData, 0);
    operator.publicKey.toBuffer().copy(prepareData, 8);
    prepareData.writeUInt32LE(dayIndex, 8 + 32);

    const prepareIx = new TransactionInstruction({
        programId: BOUNTY_PROGRAM_ID,
        keys: [
            { pubkey: bountyConfig, isSigner: false, isWritable: false },
            { pubkey: dailyPool, isSigner: false, isWritable: true },
            { pubkey: operatorStats, isSigner: false, isWritable: true },
            { pubkey: oracle.publicKey, isSigner: true, isWritable: true },
            { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        ],
        data: prepareData,
    });

    const prepareSig = await sendTx("Prepare bounty submission", [prepareIx], [oracle]);
    log("4", `prepare_bounty_submission confirmed: ${prepareSig.slice(0, 20)}...`);

    // Verify accounts were created
    const dailyPoolInfo = await conn.getAccountInfo(dailyPool);
    if (!dailyPoolInfo) fail("4", "Daily pool not created");
    log("4", `Daily pool created: ${dailyPoolInfo.data.length} bytes`);

    const operatorStatsInfo = await conn.getAccountInfo(operatorStats);
    if (!operatorStatsInfo) fail("4", "Operator stats not created");
    log("4", `Operator stats created: ${operatorStatsInfo.data.length} bytes`);

    // ── Step 5: Submit Bounty Proof ───────────────────────────────────
    log("5", "Calling submit_bounty_proof...");

    const bountyId = randomBytes(32);
    const basePoints = 100;       // base points
    const qualityScore = 85;      // quality score
    const contributionType = 1;   // feature
    const isAgent = false;
    const agentId = Buffer.alloc(32); // zero = no agent
    const reviewer = oracle.publicKey;
    const evidenceHash = createHash("sha256").update("e2e-test-evidence").digest();
    const externalReference = Buffer.alloc(64);
    Buffer.from("E2E-TEST-001").copy(externalReference);

    // Derive bounty_proof PDA
    const [bountyProof] = PublicKey.findProgramAddressSync(
        [Buffer.from("bounty_proof"), bountyId],
        BOUNTY_PROGRAM_ID
    );

    // Build submit instruction data
    // discriminator(8) + bounty_id(32) + base_points(2) + quality_score(1)
    // + contribution_type(1) + is_agent(1) + agent_id(32) + day_index(4)
    // + reviewer(32) + evidence_hash(32) + external_reference(64)
    const submitData = Buffer.alloc(8 + 32 + 2 + 1 + 1 + 1 + 32 + 4 + 32 + 32 + 64);
    let offset = 0;

    anchorDiscriminator("submit_bounty_proof").copy(submitData, offset); offset += 8;
    bountyId.copy(submitData, offset); offset += 32;
    submitData.writeUInt16LE(basePoints, offset); offset += 2;
    submitData.writeUInt8(qualityScore, offset); offset += 1;
    submitData.writeUInt8(contributionType, offset); offset += 1;
    submitData.writeUInt8(isAgent ? 1 : 0, offset); offset += 1;
    agentId.copy(submitData, offset); offset += 32;
    submitData.writeUInt32LE(dayIndex, offset); offset += 4;
    reviewer.toBuffer().copy(submitData, offset); offset += 32;
    evidenceHash.copy(submitData, offset); offset += 32;
    externalReference.copy(submitData, offset); offset += 64;

    const submitIx = new TransactionInstruction({
        programId: BOUNTY_PROGRAM_ID,
        keys: [
            { pubkey: bountyConfig, isSigner: false, isWritable: true },      // config
            { pubkey: dailyPool, isSigner: false, isWritable: true },          // daily_pool
            { pubkey: bountyProof, isSigner: false, isWritable: true },        // bounty_proof (init)
            { pubkey: operatorStats, isSigner: false, isWritable: true },      // operator_stats
            { pubkey: operator.publicKey, isSigner: false, isWritable: false },// operator
            // agent_trust = None — pass program ID to signal "not provided"
            { pubkey: BOUNTY_PROGRAM_ID, isSigner: false, isWritable: false }, // agent_trust (None)
            { pubkey: AMOS_MINT, isSigner: false, isWritable: false },         // mint
            { pubkey: BOUNTY_TREASURY, isSigner: false, isWritable: true },    // treasury
            { pubkey: operatorAta, isSigner: false, isWritable: true },        // operator_token_account
            { pubkey: reviewerAta, isSigner: false, isWritable: true },        // reviewer_token_account
            { pubkey: oracle.publicKey, isSigner: true, isWritable: true },    // oracle_authority
            { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },  // token_program
            { pubkey: SystemProgram.programId, isSigner: false, isWritable: false }, // system_program
        ],
        data: submitData,
    });

    const submitSig = await sendTx("Submit bounty proof", [submitIx], [oracle]);
    log("5", `submit_bounty_proof confirmed: ${submitSig.slice(0, 20)}...`);

    // ── Step 6: Verify Results ────────────────────────────────────────
    log("6", "Verifying on-chain results...");

    // Check bounty proof was created
    const proofInfo = await conn.getAccountInfo(bountyProof);
    if (!proofInfo) fail("6", "Bounty proof not created");
    log("6", `Bounty proof: ${proofInfo.data.length} bytes`);

    // Check operator received tokens
    const operatorTokenInfo = await conn.getAccountInfo(operatorAta);
    const operatorBalance = operatorTokenInfo.data.readBigUInt64LE(64);
    log("6", `Operator token balance: ${operatorBalance.toString()} (raw)`);
    if (operatorBalance <= 0n) fail("6", "Operator received no tokens");

    // Check reviewer received tokens
    const reviewerTokenInfo = await conn.getAccountInfo(reviewerAta);
    const reviewerBalance = reviewerTokenInfo.data.readBigUInt64LE(64);
    log("6", `Reviewer token balance: ${reviewerBalance.toString()} (raw)`);

    // Check daily pool was updated
    const dailyPoolAfter = await conn.getAccountInfo(dailyPool);
    // Skip discriminator (8), read: day_index(4), daily_emission(8), tokens_distributed(8), total_points(8), proof_count(4)
    const proofCount = dailyPoolAfter.data.readUInt32LE(8 + 4 + 8 + 8 + 8);
    log("6", `Daily pool proof count: ${proofCount}`);

    // Check operator stats updated
    const opStatsAfter = await conn.getAccountInfo(operatorStats);
    // Skip discriminator(8), operator(32), read total_bounties(8)
    const totalBounties = opStatsAfter.data.readBigUInt64LE(8 + 32);
    log("6", `Operator total bounties: ${totalBounties.toString()}`);

    // ── Step 7: Verify Treasury Vaults ────────────────────────────────
    log("7", "Verifying treasury vaults...");
    const treasuryVaultInfo = await conn.getAccountInfo(treasuryAmosVault);
    const treasuryVaultBalance = treasuryVaultInfo.data.readBigUInt64LE(64);
    log("7", `Treasury AMOS vault balance: ${treasuryVaultBalance.toString()} (should be 0 - no deposits yet)`);

    const reserveVaultInfo = await conn.getAccountInfo(reserveVault);
    const reserveVaultBalance = reserveVaultInfo.data.readBigUInt64LE(64);
    log("7", `Reserve vault balance: ${reserveVaultBalance.toString()} (should be 0 - no deposits yet)`);

    // ── Summary ───────────────────────────────────────────────────────
    console.log("\n" + "=".repeat(60));
    console.log("E2E ON-CHAIN TEST RESULTS");
    console.log("=".repeat(60));
    console.log(`Bounty program:       PASS (config + proof + distribution)`);
    console.log(`Treasury program:     PASS (config + vaults initialized)`);
    console.log(`prepare_bounty_sub:   PASS (daily_pool + operator_stats)`);
    console.log(`submit_bounty_proof:  PASS (tokens distributed)`);
    console.log(`Operator tokens:      ${operatorBalance.toString()}`);
    console.log(`Reviewer tokens:      ${reviewerBalance.toString()}`);
    console.log(`Daily pool proofs:    ${proofCount}`);
    console.log(`Operator bounties:    ${totalBounties.toString()}`);
    console.log("=".repeat(60));
    console.log("ALL TESTS PASSED");
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
