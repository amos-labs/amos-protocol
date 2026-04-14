#!/usr/bin/env bash
#
# AMOS Mainnet Deployment Script
#
# Executes the launch sequence from MAINNET_LAUNCH_PLAN.md:
#   Step 1: Deploy programs
#   Step 2: Create AMOS token mint
#   Step 3: Mint initial supply (100M)
#   Step 4: Disable mint authority (IRREVERSIBLE)
#   Step 5: Distribute allocations (95M treasury, 5M reserve)
#   Step 6: Initialize programs
#
# Each step requires confirmation before proceeding.
# Run from: amos-solana/
#
# Usage: ./scripts/mainnet-deploy.sh
#
set -euo pipefail

RPC_URL="https://api.mainnet-beta.solana.com"
KEYPAIR="$HOME/amos-founder.json"
DECIMALS=9
TOTAL_SUPPLY=100000000  # 100M AMOS
RESERVE_AMOUNT=5000000  # 5M emergency reserve

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

confirm() {
    echo ""
    echo -e "${YELLOW}$1${NC}"
    echo -n "Type 'yes' to continue: "
    read -r response
    if [ "$response" != "yes" ]; then
        echo "Aborted."
        exit 1
    fi
}

echo "============================================================"
echo "  AMOS MAINNET DEPLOYMENT"
echo "============================================================"
echo ""
echo "RPC:     $RPC_URL"
echo "Keypair: $KEYPAIR"
echo "Wallet:  $(solana address -k "$KEYPAIR")"
echo ""

# Check balance
BALANCE=$(solana balance "$(solana address -k "$KEYPAIR")" --url "$RPC_URL" | awk '{print $1}')
echo "Balance: $BALANCE SOL"
echo ""

if (( $(echo "$BALANCE < 30" | bc -l) )); then
    echo -e "${RED}WARNING: Balance is below 30 SOL. You need ~48 SOL for full deployment.${NC}"
    confirm "Continue anyway?"
fi

# ══════════════════════════════════════════════════════════════════════
# STEP 1: Deploy Programs
# ══════════════════════════════════════════════════════════════════════
echo ""
echo "============================================================"
echo "  STEP 1: Deploy Programs to Mainnet"
echo "============================================================"
echo ""
echo "Programs to deploy:"
echo "  - amos-treasury  (8ZMaZDAxDPsCnMGRkhwLmFhoG43WUJcGC8xqVKo2PN7s)"
echo "  - amos-governance (245xpoWLEAAPmUQxMSBDqQw5qnGfqt5roi5enuFG9fZZ)"
echo "  - amos-bounty    (4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq)"
echo ""
echo "This will cost ~21 SOL in rent-exempt storage."

confirm "Deploy all three programs to mainnet?"

echo ""
echo "Switching Solana config to mainnet..."
solana config set --url "$RPC_URL" --keypair "$KEYPAIR"

echo ""
echo "Building programs..."
PATH="$HOME/.cargo/bin:$PATH" anchor build

echo ""
echo "Deploying amos-treasury..."
anchor deploy --program-name amos_treasury --provider.cluster mainnet
echo -e "${GREEN}✓ amos-treasury deployed${NC}"

echo ""
echo "Deploying amos-governance..."
anchor deploy --program-name amos_governance --provider.cluster mainnet
echo -e "${GREEN}✓ amos-governance deployed${NC}"

echo ""
echo "Deploying amos-bounty..."
anchor deploy --program-name amos_bounty --provider.cluster mainnet
echo -e "${GREEN}✓ amos-bounty deployed${NC}"

BALANCE=$(solana balance --url "$RPC_URL" | awk '{print $1}')
echo ""
echo "Remaining balance: $BALANCE SOL"

# ══════════════════════════════════════════════════════════════════════
# STEP 2: Create Token Mint
# ══════════════════════════════════════════════════════════════════════
echo ""
echo "============================================================"
echo "  STEP 2: Create AMOS Token Mint ($DECIMALS decimals)"
echo "============================================================"

confirm "Create the AMOS SPL token mint on mainnet?"

MINT_OUTPUT=$(spl-token create-token --decimals "$DECIMALS" --url "$RPC_URL" --fee-payer "$KEYPAIR" 2>&1)
echo "$MINT_OUTPUT"
MINT_ADDRESS=$(echo "$MINT_OUTPUT" | grep -oP 'Creating token \K\S+' || echo "$MINT_OUTPUT" | grep -oP 'Address:\s+\K\S+' || true)

if [ -z "$MINT_ADDRESS" ]; then
    echo -e "${YELLOW}Could not auto-parse mint address from output above.${NC}"
    echo -n "Paste the mint address: "
    read -r MINT_ADDRESS
fi

echo ""
echo -e "${GREEN}AMOS Mint Address: $MINT_ADDRESS${NC}"
echo ""
echo "SAVE THIS ADDRESS — it is the AMOS token on mainnet."

# ══════════════════════════════════════════════════════════════════════
# STEP 3: Mint Initial Supply
# ══════════════════════════════════════════════════════════════════════
echo ""
echo "============================================================"
echo "  STEP 3: Mint $TOTAL_SUPPLY AMOS (total supply)"
echo "============================================================"

echo "Creating token account..."
spl-token create-account "$MINT_ADDRESS" --url "$RPC_URL" --fee-payer "$KEYPAIR"

confirm "Mint $TOTAL_SUPPLY AMOS tokens to your wallet?"

spl-token mint "$MINT_ADDRESS" "$TOTAL_SUPPLY" --url "$RPC_URL" --fee-payer "$KEYPAIR"
echo -e "${GREEN}✓ Minted $TOTAL_SUPPLY AMOS${NC}"

echo ""
echo "Token balance:"
spl-token balance "$MINT_ADDRESS" --url "$RPC_URL"

# ══════════════════════════════════════════════════════════════════════
# STEP 4: Disable Mint Authority
# ══════════════════════════════════════════════════════════════════════
echo ""
echo "============================================================"
echo "  STEP 4: Disable Mint Authority"
echo "============================================================"
echo ""
echo -e "${RED}╔══════════════════════════════════════════════════════════╗${NC}"
echo -e "${RED}║  THIS IS IRREVERSIBLE                                   ║${NC}"
echo -e "${RED}║  No more AMOS tokens can ever be created after this.    ║${NC}"
echo -e "${RED}║  Fixed supply: 100,000,000 AMOS forever.               ║${NC}"
echo -e "${RED}╚══════════════════════════════════════════════════════════╝${NC}"

confirm "Permanently disable mint authority? THIS CANNOT BE UNDONE."

spl-token authorize "$MINT_ADDRESS" mint --disable --url "$RPC_URL" --fee-payer "$KEYPAIR"
echo -e "${GREEN}✓ Mint authority disabled permanently${NC}"

# ══════════════════════════════════════════════════════════════════════
# STEP 5: Distribute Allocations
# ══════════════════════════════════════════════════════════════════════
echo ""
echo "============================================================"
echo "  STEP 5: Distribute Initial Allocations"
echo "============================================================"
echo ""
echo "Emergency Reserve: $RESERVE_AMOUNT AMOS (5%)"
echo "Bounty Treasury:   $(( TOTAL_SUPPLY - RESERVE_AMOUNT )) AMOS (95%) — stays in wallet until treasury program is initialized"

confirm "Transfer $RESERVE_AMOUNT AMOS to a reserve wallet? (You'll need the reserve wallet address)"

echo -n "Enter the reserve wallet address (or press Enter to skip for now): "
read -r RESERVE_WALLET

if [ -n "$RESERVE_WALLET" ]; then
    spl-token transfer "$MINT_ADDRESS" "$RESERVE_AMOUNT" "$RESERVE_WALLET" --url "$RPC_URL" --fee-payer "$KEYPAIR" --fund-recipient
    echo -e "${GREEN}✓ $RESERVE_AMOUNT AMOS transferred to reserve${NC}"
else
    echo "Skipping reserve transfer — do this manually before launch."
fi

# ══════════════════════════════════════════════════════════════════════
# STEP 6: Initialize Programs
# ══════════════════════════════════════════════════════════════════════
echo ""
echo "============================================================"
echo "  STEP 6: Initialize Programs"
echo "============================================================"
echo ""
echo "Mint address for initialization: $MINT_ADDRESS"

confirm "Initialize Treasury, Governance, and Bounty programs with this mint?"

echo ""
echo "Initializing Treasury..."
AMOS_MINT="$MINT_ADDRESS" NETWORK=mainnet node scripts/initialize-treasury.mjs
echo -e "${GREEN}✓ Treasury initialized${NC}"

echo ""
echo "Initializing Bounty..."
AMOS_MINT="$MINT_ADDRESS" NETWORK=mainnet node scripts/initialize-bounty.mjs
echo -e "${GREEN}✓ Bounty initialized${NC}"

# ══════════════════════════════════════════════════════════════════════
# DONE
# ══════════════════════════════════════════════════════════════════════
echo ""
echo "============================================================"
echo -e "${GREEN}  MAINNET DEPLOYMENT COMPLETE${NC}"
echo "============================================================"
echo ""
echo "  AMOS Mint:     $MINT_ADDRESS"
echo "  Treasury:      8ZMaZDAxDPsCnMGRkhwLmFhoG43WUJcGC8xqVKo2PN7s"
echo "  Governance:    245xpoWLEAAPmUQxMSBDqQw5qnGfqt5roi5enuFG9fZZ"
echo "  Bounty:        4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq"
echo ""
echo "NEXT STEPS:"
echo "  1. Fund treasury token vault with 95M AMOS"
echo "  2. Create Raydium AMOS/SOL LP pool"
echo "  3. Update relay/harness env vars with mainnet mint"
echo "  4. Post first mainnet bounty"
echo "  5. Update README with mainnet addresses"
echo ""
echo "  Save this mint address in your configs:"
echo "  export AMOS_MAINNET_MINT=$MINT_ADDRESS"
echo ""
