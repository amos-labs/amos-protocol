# Connecting a Solana Wallet to AMOS

This guide walks you through setting up a Solana wallet and connecting it to your AMOS harness. Once connected, you can claim bounties, earn AMOS tokens, and participate in the relay economy.

**Time required:** 2-3 minutes with email signup, 5-10 minutes with a self-custodial wallet. Under a minute if you already have a wallet.

---

## What You Need

- A modern browser (Chrome, Brave, Firefox, or Edge)
- Access to your AMOS harness (your organization's URL)
- A Solana wallet — either **Phantom** or **Solflare**

AMOS supports both wallets equally. If you don't have a preference, Phantom is the most widely used Solana wallet and a good default. Phantom lets you create a wallet with just an email address — no crypto experience required.

---

## Step 1: Set Up a Wallet

*Skip this step if you already have Phantom or Solflare installed.*

### Option A: Phantom with Email (Fastest)

This is the easiest path if you've never used a crypto wallet before. Phantom creates and manages a wallet for you — no seed phrases, no key management.

1. Go to [phantom.app](https://phantom.app) and click **Download**
2. Select your browser (Chrome, Brave, Firefox, or Edge) — this takes you to the browser's extension store
3. Click **Add to Browser** and confirm the install
4. Phantom opens automatically. Choose **Sign up with email** (or Google)
5. Enter your email and verify it — Phantom creates a wallet for you behind the scenes
6. Set a password to lock the extension on your device
7. Your wallet is ready. You'll see it as a small icon in your browser's extension toolbar.

That's it. You have a Solana wallet address and can connect to AMOS immediately. If you want to move to full self-custody later, Phantom lets you export your keys at any time from Settings.

### Option B: Phantom with Recovery Phrase (Self-Custodial)

Choose this if you want full control of your keys from the start.

1. Go to [phantom.app](https://phantom.app) and click **Download**
2. Select your browser and install the extension
3. Phantom opens automatically. Choose **Create a New Wallet**
4. Set a password — this unlocks the extension on your device
5. **Write down your recovery phrase.** This is 12 words that are the only way to recover your wallet if you lose access to your device. Store it somewhere safe and offline. Never share it with anyone, and never enter it into a website.
6. Confirm the recovery phrase when prompted
7. Your wallet is ready.

### Option C: Solflare

1. Go to [solflare.com](https://solflare.com) and click **Download**
2. Select your browser and install the extension
3. Choose **Create a New Wallet** and set a password
4. **Write down your recovery phrase** — same rules as above. 12 words, offline, never share.
5. Confirm and finish setup

---

## Step 2: Connect Your Wallet to AMOS

1. Log into your AMOS harness
2. Look for the **wallet icon** in the left sidebar — it has a small gray dot next to it, indicating no wallet is connected yet
3. Click the wallet icon to open wallet settings
4. Click **Connect Wallet**
5. A modal appears showing the wallet extensions detected in your browser. Click **Phantom** or **Solflare**
6. Your wallet extension pops up asking you to approve the connection to the AMOS site. Click **Connect**
7. Immediately after connecting, AMOS asks your wallet to **sign a verification message**. This proves you own the wallet — it does NOT spend any tokens or authorize any transactions. The message looks like: `AMOS wallet verification: <your-address> at <timestamp>`. Click **Approve** or **Sign**
8. Done. The wallet icon in the sidebar now shows a **green dot**. Your wallet address appears in the settings panel.

---

## Step 3: Verify Your Connection

After connecting, you should see:

- **Green dot** on the wallet icon in the sidebar
- Your **wallet address** displayed in settings (truncated, like `ABcD...xYzW`) — click the copy icon to get the full address
- Your **AMOS token balance** (will show 0.00 until you earn tokens)

If any of these are missing, try disconnecting and reconnecting from the wallet settings panel.

---

## Earning AMOS Tokens

With your wallet connected, you can now participate in the bounty economy:

- **Browse bounties** in the AMOS bounty board
- **Claim a bounty** you can complete — this registers your wallet address as the claimant
- **Submit your work** with proof of completion
- **Receive tokens** directly to your connected wallet when your submission is approved

Tokens are transferred on-chain from the AMOS treasury to your wallet address. You can verify any transaction on [Solscan](https://solscan.io) by searching your wallet address.

---

## Managing Your Wallet

**Disconnect:** Open wallet settings (click the wallet icon in the sidebar) and click **Disconnect**. This unlinks your wallet from AMOS but does NOT affect your wallet or any tokens in it.

**Switch wallets:** Disconnect your current wallet first, then connect a different one. AMOS supports one wallet per account.

**Check balance:** Your AMOS token balance is shown in wallet settings and refreshes automatically. For a full transaction history, search your wallet address on [Solscan](https://solscan.io).

---

## Troubleshooting

**"No wallets detected"** — Make sure Phantom or Solflare is installed and enabled in your browser. Some browsers require you to click the extensions icon (puzzle piece) and pin the wallet extension. Refresh the AMOS page after installing.

**Wallet popup doesn't appear** — Your browser may be blocking popups from the wallet extension. Check your browser's popup/notification settings. Also try clicking the wallet extension icon directly to make sure it's unlocked.

**"Failed to register wallet"** — This usually means the signature verification failed. Try disconnecting and reconnecting. If the problem persists, make sure your system clock is accurate (the verification uses a timestamp-based nonce that expires after 5 minutes).

**Balance shows 0 after earning tokens** — Token transfers happen on-chain and may take a few seconds to confirm. Refresh the page or wait for the automatic balance refresh (every 60 seconds). You can also check [Solscan](https://solscan.io) to verify the transaction.

**"Wallet already connected to another account"** — Each wallet can only be linked to one AMOS account. If you previously connected this wallet to a different account, you'll need to disconnect it from that account first, or use a different wallet.

---

## Security Notes

- AMOS **never** has access to your private keys or recovery phrase. The wallet extension handles all signing locally in your browser.
- Connecting your wallet only proves ownership — it does not authorize AMOS to move your tokens or sign transactions on your behalf.
- Every token transfer requires an on-chain transaction signed by the AMOS treasury program, not by your wallet. Your tokens are yours.
- If you see any site other than your organization's AMOS URL asking you to connect your wallet or sign messages, do not approve it.

**A note on email wallets vs self-custodial wallets:** If you created your wallet with Phantom's email signup, Phantom manages your keys using multi-party computation (MPC). This is convenient but means you're trusting Phantom with a share of your key. For most users earning AMOS through bounties, this is a fine tradeoff. If you accumulate significant token holdings and want full self-custody, you can export your private key from Phantom's settings and import it into a self-custodial wallet at any time.
