import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { PublicKey } from '@solana/web3.js';
import { addresses, decodePoolV2, decodeBountyConfig, parseArgs, buildInstruction, TREASURY_PROGRAM_ID, BOUNTY_PROGRAM_ID } from '../../../scripts/set-fee-recipients.mjs';
const hash = (name) => createHash('sha256').update(`account:${name}`).digest().subarray(0, 8);
const key = (n) => new PublicKey(Buffer.alloc(32, n));
test('dry-run accepts no key or implied default RPC; execute is explicit', () => {
  assert.equal(parseArgs(['--rpc', 'http://127.0.0.1:8899', '--labs-token', key(1).toBase58()]).execute, false);
  assert.throws(() => parseArgs([]));
  assert.throws(() => parseArgs(['--rpc', 'local', '--labs-token', 'token', '--keypair', 'do-not-read']));
  assert.throws(() => parseArgs(['--rpc', 'local', '--labs-token', 'token', '--execute']));
});
test('V2 decoding refuses legacy data, incorrect owner and version', () => {
  const data = Buffer.alloc(130); hash('RevenuePoolV2').copy(data); data[8] = 2; key(1).toBuffer().copy(data, 9); key(2).toBuffer().copy(data, 41);
  assert.equal(decodePoolV2({ owner: TREASURY_PROGRAM_ID, data }).mint.toBase58(), key(1).toBase58());
  assert.throws(() => decodePoolV2({ owner: BOUNTY_PROGRAM_ID, data }));
  assert.throws(() => decodePoolV2({ owner: TREASURY_PROGRAM_ID, data: Buffer.alloc(130) }));
  data[8] = 1; assert.throws(() => decodePoolV2({ owner: TREASURY_PROGRAM_ID, data }));
});
test('bounty recipient decoder uses current discriminator and exact legacy layout', () => {
  const data = Buffer.alloc(276); hash('BountyConfig').copy(data); key(1).toBuffer().copy(data, 148); key(2).toBuffer().copy(data, 180);
  const result = decodeBountyConfig({ owner: BOUNTY_PROGRAM_ID, data });
  assert.equal(result.holderPool.toBase58(), key(1).toBase58()); assert.equal(result.labsTokens.toBase58(), key(2).toBase58());
  assert.throws(() => decodeBountyConfig({ owner: BOUNTY_PROGRAM_ID, data: data.subarray(0, 275) }));
});
test('builder routes holder fees to canonical V2 custody and retains required oracle signer', () => {
  const ix = buildInstruction({ mint: key(1), labsTokens: key(2), oracle: key(3) });
  assert.equal(ix.keys.length, 5); assert.ok(ix.keys[2].pubkey.equals(addresses().rewards));
  assert.equal(ix.keys[4].isSigner, true); assert.ok(ix.keys[4].pubkey.equals(key(3)));
  assert.ok(!addresses().rewards.equals(PublicKey.findProgramAddressSync([Buffer.from('treasury_amos')], TREASURY_PROGRAM_ID)[0]));
});
