/**
 * Configure commercial holder fees for treasury V2; read/dry-run by default.
 * node scripts/set-fee-recipients.mjs --rpc URL --labs-token TOKEN_ACCOUNT
 * Add --execute --keypair PATH only after the V2 migration/deployment review.
 * No RPC endpoint, mint, signing key, or Labs token address is guessed.
 */
import { Connection, Keypair, PublicKey, Transaction, TransactionInstruction } from '@solana/web3.js';
import { getAccount, getMint, TOKEN_PROGRAM_ID } from '@solana/spl-token';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';

export const BOUNTY_PROGRAM_ID = new PublicKey('4XbUwKNMoERKuzzeSKJgATttgHFcjazohuYYgiwj9tsq');
export const TREASURY_PROGRAM_ID = new PublicKey('8ZMaZDAxDPsCnMGRkhwLmFhoG43WUJcGC8xqVKo2PN7s');
const discriminator = (name) => createHash('sha256').update(name).digest().subarray(0, 8);
const pda = (seed, program) => PublicKey.findProgramAddressSync([Buffer.from(seed)], program)[0];
export function addresses() {
  return { bountyConfig: pda('bounty_config', BOUNTY_PROGRAM_ID), pool: pda('revenue_pool_v2', TREASURY_PROGRAM_ID), rewards: pda('holder_rewards_v2', TREASURY_PROGRAM_ID) };
}
export function decodeBountyConfig(info) {
  if (!info?.owner.equals(BOUNTY_PROGRAM_ID) || info.data.length !== 276 || !info.data.subarray(0, 8).equals(discriminator('account:BountyConfig'))) throw new Error('Unsupported bounty config owner/layout');
  return { oracle: new PublicKey(info.data.subarray(8, 40)), mint: new PublicKey(info.data.subarray(40, 72)), holderPool: new PublicKey(info.data.subarray(148, 180)), labsTokens: new PublicKey(info.data.subarray(180, 212)) };
}
export function decodePoolV2(info) {
  if (!info?.owner.equals(TREASURY_PROGRAM_ID) || info.data.length !== 130 || !info.data.subarray(0, 8).equals(discriminator('account:RevenuePoolV2')) || info.data[8] !== 2) throw new Error('Missing or unsupported V2 pool; do not use legacy holder accounting');
  return { mint: new PublicKey(info.data.subarray(9, 41)), labsWallet: new PublicKey(info.data.subarray(41, 73)) };
}
export function buildInstruction({ mint, labsTokens, oracle }) {
  const a = addresses();
  return new TransactionInstruction({ programId: BOUNTY_PROGRAM_ID, data: discriminator('global:set_fee_recipients'), keys: [
    { pubkey: a.bountyConfig, isSigner: false, isWritable: true },
    { pubkey: mint, isSigner: false, isWritable: false },
    { pubkey: a.rewards, isSigner: false, isWritable: false },
    { pubkey: labsTokens, isSigner: false, isWritable: false },
    { pubkey: oracle, isSigner: true, isWritable: false },
  ] });
}
export function parseArgs(args) {
  const out = { execute: false };
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--execute') { if (out.execute) throw new Error('Duplicate --execute'); out.execute = true; }
    else if (['--rpc', '--labs-token', '--keypair'].includes(arg)) {
      const key = arg.slice(2); if (out[key] || !args[i + 1] || args[i + 1].startsWith('--')) throw new Error(`Missing/duplicate ${arg}`);
      out[key] = args[++i];
    } else throw new Error(`Unknown argument: ${arg}`);
  }
  if (!out.rpc || !out['labs-token']) throw new Error('Required: --rpc URL --labs-token TOKEN_ACCOUNT');
  if (out.execute && !out.keypair) throw new Error('--execute requires an explicit --keypair');
  if (!out.execute && out.keypair) throw new Error('Dry-run does not accept a signing key');
  return out;
}
export async function main(args) {
  const options = parseArgs(args);
  const connection = new Connection(options.rpc, 'confirmed');
  const a = addresses();
  const [configInfo, poolInfo] = await connection.getMultipleAccountsInfo([a.bountyConfig, a.pool]);
  const config = decodeBountyConfig(configInfo); const pool = decodePoolV2(poolInfo);
  if (!config.mint.equals(pool.mint)) throw new Error('Bounty and V2 pool mints differ');
  const labsTokens = new PublicKey(options['labs-token']);
  const [mint, rewards, labs] = await Promise.all([
    getMint(connection, pool.mint, 'confirmed', TOKEN_PROGRAM_ID),
    getAccount(connection, a.rewards, 'confirmed', TOKEN_PROGRAM_ID),
    getAccount(connection, labsTokens, 'confirmed', TOKEN_PROGRAM_ID),
  ]);
  if (mint.decimals !== 9 || !rewards.mint.equals(pool.mint) || !rewards.owner.equals(a.pool) || rewards.delegate || rewards.closeAuthority) throw new Error('V2 reward custody/mint mismatch');
  if (!labs.mint.equals(pool.mint) || !labs.owner.equals(pool.labsWallet)) throw new Error('Labs token account is not owned by the immutable V2 Labs wallet');
  const ix = buildInstruction({ mint: pool.mint, labsTokens, oracle: config.oracle });
  console.log(JSON.stringify({ mode: options.execute ? 'execute' : 'dry-run', mint: pool.mint.toBase58(), oracle: config.oracle.toBase58(), holderPool: a.rewards.toBase58(), labsTokens: labsTokens.toBase58(), previousHolderPool: config.holderPool.toBase58(), previousLabsTokens: config.labsTokens.toBase58(), instruction: { programId: ix.programId.toBase58(), dataHex: ix.data.toString('hex'), accounts: ix.keys.map(({ pubkey, ...flags }) => ({ address: pubkey.toBase58(), ...flags })) } }, null, 2));
  if (!options.execute) return;
  const oracle = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(options.keypair, 'utf8'))));
  if (!oracle.publicKey.equals(config.oracle)) throw new Error('Signing key is not the configured oracle');
  const latest = await connection.getLatestBlockhash('confirmed');
  const tx = new Transaction({ feePayer: oracle.publicKey, ...latest }).add(ix); tx.sign(oracle);
  const signature = await connection.sendRawTransaction(tx.serialize(), { skipPreflight: false, preflightCommitment: 'confirmed' });
  const confirmation = await connection.confirmTransaction({ signature, ...latest }, 'confirmed');
  if (confirmation.value.err) throw new Error(`Transaction failed: ${JSON.stringify(confirmation.value.err)}`);
  const after = decodeBountyConfig(await connection.getAccountInfo(a.bountyConfig, 'confirmed'));
  if (!after.holderPool.equals(a.rewards) || !after.labsTokens.equals(labsTokens)) throw new Error('Post-write fee recipients do not match requested configuration');
  console.log(JSON.stringify({ signature, verified: true }));
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main(process.argv.slice(2)).catch((error) => { console.error(error.message); process.exitCode = 1; });
