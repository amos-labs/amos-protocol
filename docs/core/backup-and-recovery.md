# Database Backup And Recovery

Operational guidance for backing up AMOS data stores and recovering from
data loss. Targets both the per-customer harness deployment
(`docker/harness/docker-compose.yml`) and the relay.

## What Needs Backing Up

| Store | Contents | Loss impact | Backup? |
|-------|----------|-------------|---------|
| Harness PostgreSQL (`amos_harness`) | Sessions, messages, collections/records, canvases, sites, uploads metadata, memory embeddings (pgvector) | Customer data loss — unrecoverable | Yes — primary target |
| Relay PostgreSQL (`amos_relay`) | Bounties, claims, proof receipts, reputation, settlement state | Economic state loss; on-chain settlements survive but off-chain lifecycle does not | Yes — primary target |
| Upload blobs (storage backend) | Raw uploaded files (`uploads.storage_key` points here) | Files referenced by DB rows become 404s | Yes — alongside the DB |
| Redis | Cache, transient queues | Cold cache; rebuilt automatically | No (ephemeral) |
| Solana keypairs (`oracle_keypair_path`) | Settlement signing keys | Settlement halts; keys are NOT in the DB | Yes — secrets manager / offline, never in DB dumps |

The two PostgreSQL databases and the upload blob store must be backed up
**as a set**: upload metadata without blobs (or vice versa) is a partial
restore.

## Recommended Setup

### Managed (production default)

Run PostgreSQL on a managed service (e.g. RDS) as already recommended in
`docker-compose.yml` production notes:

- Enable automated daily snapshots, retention ≥ 14 days.
- Enable point-in-time recovery (WAL-based, 5-minute RPO typical).
- Verify the instance uses encrypted storage; snapshots inherit encryption.
- For uploads on S3: enable bucket versioning plus a lifecycle rule that
  expires noncurrent versions after the same retention window.

### Self-hosted (docker compose)

For deployments using the bundled Postgres 16 + pgvector containers:

1. **Nightly logical dumps** from the host (cron or systemd timer):

   ```bash
   docker compose exec -T postgres \
     pg_dump -U "$POSTGRES_USER" -Fc amos_harness \
     > backups/amos_harness_$(date +%F).dump
   ```

   `-Fc` (custom format) supports parallel and selective restore. Repeat
   per database (`amos_relay` on the relay host). The pgvector extension
   is restored automatically as long as the target image includes it.

2. **WAL archiving for PITR** (optional, for tighter RPO): set
   `archive_mode = on` and ship WAL segments off-host (e.g.
   `wal-g`/`pgbackrest`). Without it, worst-case data loss is the time
   since the last nightly dump (≈24h RPO).

3. **Do not rely on volume snapshots of a running container** — copying
   `postgres_data` while Postgres is writing produces inconsistent
   backups unless the snapshot is filesystem-atomic. Prefer `pg_dump` /
   WAL archiving.

4. **Local upload storage**: rsync the uploads directory after the DB
   dump completes, so blob state is never older than the metadata that
   references it.

### Retention and encryption

- Keep 14 daily, 8 weekly, 12 monthly backups (adjust per customer
  contract). Prune automatically.
- Encrypt dumps at rest (`age`, `gpg`, or server-side encryption on the
  backup bucket) and in transit (TLS to the backup target).
- Backups contain customer data and integration credentials
  (`integrations.credentials` JSONB) — treat dump files with the same
  access controls as the production database.
- Never include Solana keypair files in DB backup bundles; manage them in
  a secrets manager with separate, audited access.

## Restore Procedure

1. Provision a Postgres 16 instance **with the pgvector extension
   image** (`pgvector/pgvector:pg16` or equivalent).
2. Create the database and restore:

   ```bash
   createdb -U amos amos_harness
   pg_restore -U amos -d amos_harness --no-owner backups/amos_harness_YYYY-MM-DD.dump
   ```

3. Restore the upload blob store to the path/bucket the harness config
   points at.
4. Start the harness. sqlx migrations run on startup and are idempotent
   against a restored schema (`_sqlx_migrations` is part of the dump).
5. Smoke-check: `GET /health`, open a chat session, fetch a previously
   uploaded file, list collections.

For PITR restores, recover to the target timestamp first, then run steps
3–5.

## Verification Drills

A backup that has never been restored is a hope, not a backup.

- **Monthly**: restore the latest dump into a scratch container, run the
  smoke checks above, record the result.
- **After schema changes**: run one drill before promoting a release that
  adds migrations, confirming dumps from the previous version restore and
  migrate cleanly.
- Alert if the newest backup is older than 26 hours (one missed nightly).

## Targets

| Metric | Managed (RDS + PITR) | Self-hosted (nightly dump) |
|--------|----------------------|----------------------------|
| RPO (max data loss) | ≤ 5 minutes | ≤ 24 hours |
| RTO (time to restore) | ≤ 1 hour | ≤ 4 hours |

If a customer needs a tighter RPO than nightly dumps provide, move that
deployment to the managed setup or add WAL archiving rather than
increasing dump frequency.
