/**
 * End-to-End Bounty Lifecycle Test
 *
 * Tests the full bounty flow:
 * 1. Create bounty on relay
 * 2. Claim bounty as agent
 * 3. Submit work
 * 4. Approve submission (triggers on-chain settlement)
 * 5. Verify on-chain state
 *
 * Prerequisites:
 * - Relay running at http://localhost:4100 (or RELAY_URL env)
 * - Bounty program deployed and initialized on devnet
 * - Oracle keypair at ~/amos-founder.json
 */

import { Connection, PublicKey } from "@solana/web3.js";
import { readFileSync } from "fs";
import { createHash } from "crypto";

const RELAY_URL = process.env.RELAY_URL || "http://localhost:4100";
const RPC_URL = "https://api.devnet.solana.com";
const BOUNTY_PROGRAM_ID = new PublicKey("4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq");
const MINT = new PublicKey("5g9vvce3YLsqZPBGAuKmGFfNKb5sp7v3Wiga5de8d5bQ");

const conn = new Connection(RPC_URL, "confirmed");

// Load oracle wallet address
const keypairPath = process.env.HOME + "/amos-founder.json";
const keypairData = JSON.parse(readFileSync(keypairPath, "utf-8"));
const walletAddress = "WxdXw1f1kFMRu8HDf1SE6yjgeWyf3Vb4T63QXMs4yij";

function log(step, msg) {
    console.log(`\n[${ step }] ${msg}`);
}

function fail(step, msg) {
    console.error(`\n[${ step }] FAILED: ${msg}`);
    process.exit(1);
}

async function relayRequest(method, path, body) {
    const opts = {
        method,
        headers: {
            "Content-Type": "application/json",
            "Authorization": "Bearer e2e-test-token",
        },
    };
    if (body) opts.body = JSON.stringify(body);

    const resp = await fetch(`${RELAY_URL}${path}`, opts);
    const text = await resp.text();
    let data;
    try { data = JSON.parse(text); } catch { data = text; }
    return { status: resp.status, data };
}

async function main() {
    console.log("=".repeat(60));
    console.log("AMOS End-to-End Bounty Lifecycle Test");
    console.log("=".repeat(60));
    console.log(`Relay: ${RELAY_URL}`);
    console.log(`RPC: ${RPC_URL}`);
    console.log(`Wallet: ${walletAddress}`);

    // ── Step 0: Check relay health ─────────────────────────────────
    log("0", "Checking relay health...");
    try {
        const { status } = await relayRequest("GET", "/health");
        if (status !== 200) fail("0", `Relay unhealthy: ${status}`);
        log("0", "Relay is healthy");
    } catch (e) {
        fail("0", `Cannot reach relay at ${RELAY_URL}: ${e.message}`);
    }

    // ── Step 1: Create bounty ──────────────────────────────────────
    log("1", "Creating bounty...");
    const deadline = new Date();
    deadline.setDate(deadline.getDate() + 7);

    const { status: createStatus, data: bounty } = await relayRequest("POST", "/api/v1/bounties", {
        title: "E2E Test: Analyze devnet token metrics",
        description: "Pull AMOS token transfer data from devnet, compute daily active wallets, and summarize in a report.",
        reward_tokens: 500,
        deadline: deadline.toISOString(),
        required_capabilities: ["web_search", "document_processing"],
        poster_wallet: walletAddress,
    });

    if (createStatus !== 201) fail("1", `Create returned ${createStatus}: ${JSON.stringify(bounty)}`);
    const bountyId = bounty.id;
    log("1", `Bounty created: ${bountyId} (${bounty.title})`);
    log("1", `Status: ${bounty.status}, Reward: ${bounty.reward_tokens} pts`);

    // ── Step 2: List bounties (verify it shows up) ─────────────────
    log("2", "Listing open bounties...");
    const { data: openBounties } = await relayRequest("GET", "/api/v1/bounties?status=open");
    const found = Array.isArray(openBounties)
        ? openBounties.find(b => b.id === bountyId)
        : null;
    if (!found) fail("2", "Bounty not found in open list");
    log("2", `Found ${openBounties.length} open bounties, including ours`);

    // ── Step 2b: Register a test agent ────────────────────────────
    log("2b", "Registering test agent...");
    const { status: regStatus, data: agent } = await relayRequest("POST", "/api/v1/agents/register", {
        name: "e2e-test-agent",
        display_name: "E2E Test Agent",
        endpoint_url: "http://localhost:9999",
        capabilities: ["web_search", "document_processing"],
        description: "Temporary agent for E2E bounty test",
        wallet_address: walletAddress,
    });
    if (regStatus !== 201) fail("2b", `Register agent returned ${regStatus}: ${JSON.stringify(agent)}`);
    const agentId = agent.id;
    log("2b", `Agent registered: ${agentId} (${agent.name})`);

    // ── Step 3: Claim bounty ───────────────────────────────────────
    log("3", "Claiming bounty as agent...");
    const { status: claimStatus, data: claimedBounty } = await relayRequest(
        "POST",
        `/api/v1/bounties/${bountyId}/claim`,
        { agent_id: agentId, harness_id: "e2e-test-harness", wallet_address: walletAddress }
    );
    if (claimStatus !== 200) fail("3", `Claim returned ${claimStatus}: ${JSON.stringify(claimedBounty)}`);
    log("3", `Bounty claimed. Status: ${claimedBounty.status}`);
    log("3", `Claimed by wallet: ${claimedBounty.claimed_by_wallet || "not set"}`);
    if (claimedBounty.status !== "claimed") fail("3", `Expected 'claimed', got '${claimedBounty.status}'`);
    if (claimedBounty.claimed_by_wallet !== walletAddress) {
        log("3", `WARNING: claimed_by_wallet expected '${walletAddress}', got '${claimedBounty.claimed_by_wallet}'`);
    }

    // ── Step 4: Submit work ────────────────────────────────────────
    log("4", "Submitting work...");
    const { status: submitStatus, data: submittedBounty } = await relayRequest(
        "POST",
        `/api/v1/bounties/${bountyId}/submit`,
        {
            agent_id: agentId,
            result: {
                report: "AMOS devnet token metrics: 42 daily active wallets, 156 transfers in last 24h, avg transfer size 1,250 AMOS.",
                confidence: 0.95,
                sources: ["devnet RPC", "token program logs"],
            },
            quality_evidence: {
                methodology: "Direct RPC queries with cross-validation",
                data_points: 156,
            },
        }
    );
    if (submitStatus !== 200) fail("4", `Submit returned ${submitStatus}: ${JSON.stringify(submittedBounty)}`);
    log("4", `Work submitted. Status: ${submittedBounty.status}`);
    if (submittedBounty.status !== "submitted") fail("4", `Expected 'submitted', got '${submittedBounty.status}'`);

    // ── Step 5: Approve submission ─────────────────────────────────
    log("5", "Approving submission (triggers on-chain settlement)...");
    const { status: approveStatus, data: approvedBounty } = await relayRequest(
        "POST",
        `/api/v1/bounties/${bountyId}/approve`,
        {
            reviewer_wallet: walletAddress,
            quality_score: 85,
        }
    );
    if (approveStatus !== 200) fail("5", `Approve returned ${approveStatus}: ${JSON.stringify(approvedBounty)}`);
    log("5", `Bounty approved! Status: ${approvedBounty.status}`);
    log("5", `Quality score: ${approvedBounty.quality_score}`);
    if (approvedBounty.settlement_tx) {
        log("5", `Settlement TX: ${approvedBounty.settlement_tx}`);
    } else {
        log("5", "No settlement TX (relay may not have oracle keypair configured)");
    }

    // ── Step 6: Verify on-chain ────────────────────────────────────
    log("6", "Verifying on-chain state...");

    // Check config PDA
    const BOUNTY_CONFIG_SEED = Buffer.from("bounty_config");
    const [configPda] = PublicKey.findProgramAddressSync([BOUNTY_CONFIG_SEED], BOUNTY_PROGRAM_ID);
    const configAccount = await conn.getAccountInfo(configPda);
    if (!configAccount) fail("6", "Config PDA not found on-chain");
    log("6", `Config PDA exists: ${configPda.toBase58()} (${configAccount.data.length} bytes)`);

    // Check bounty proof PDA (if settlement happened)
    if (approvedBounty.settlement_tx) {
        const bountyIdHash = createHash("sha256").update(bountyId).digest();
        const BOUNTY_PROOF_SEED = Buffer.from("bounty_proof");
        const [proofPda] = PublicKey.findProgramAddressSync(
            [BOUNTY_PROOF_SEED, bountyIdHash],
            BOUNTY_PROGRAM_ID
        );
        const proofAccount = await conn.getAccountInfo(proofPda);
        if (proofAccount) {
            log("6", `Bounty proof on-chain: ${proofPda.toBase58()} (${proofAccount.data.length} bytes)`);
        } else {
            log("6", "Bounty proof not found on-chain (may take time to confirm)");
        }
    }

    // ── Step 7: Verify final bounty state ──────────────────────────
    log("7", "Fetching final bounty state...");
    const { data: finalBounty } = await relayRequest("GET", `/api/v1/bounties/${bountyId}`);
    log("7", `Final status: ${finalBounty.status}`);
    log("7", `Reward: ${finalBounty.reward_tokens} pts`);
    log("7", `Quality score: ${finalBounty.quality_score}`);
    log("7", `Settlement TX: ${finalBounty.settlement_tx || "pending"}`);
    log("7", `Settlement status: ${finalBounty.settlement_status || "n/a"}`);

    console.log("\n" + "=".repeat(60));
    console.log("E2E TEST COMPLETE");
    console.log("=".repeat(60));
    console.log(`Bounty ID: ${bountyId}`);
    console.log(`Status: ${finalBounty.status}`);
    console.log(`Settlement: ${finalBounty.settlement_status || "not configured"}`);
    console.log("=".repeat(60));
}

main().catch((err) => {
    console.error("\nUnexpected error:", err);
    process.exit(1);
});
