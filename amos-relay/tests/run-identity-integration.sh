#!/usr/bin/env bash
set -euo pipefail
# New local-only services and disposable data. Never uses DATABASE_URL or an
# existing Redis instance. The Rust test also checks the dedicated URL shape.
ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
TEST_DIR="$(mktemp -d "${TMPDIR:-/tmp}/amos-relay-identity.XXXXXX")"
free_port() { python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()'; }
PG_PORT="$(free_port)"
REDIS_PORT="$(free_port)"
cleanup() {
  pg_ctl -D "$TEST_DIR/pg" -m immediate stop >/dev/null 2>&1 || true
  if [[ -f "$TEST_DIR/redis.pid" ]]; then
    local redis_pid
    redis_pid="$(cat "$TEST_DIR/redis.pid")"
    redis-cli -h 127.0.0.1 -p "$REDIS_PORT" shutdown nosave >/dev/null 2>&1 || kill "$redis_pid" 2>/dev/null || true
    for _ in {1..100}; do
      if ! kill -0 "$redis_pid" 2>/dev/null; then break; fi
      sleep 0.1
    done
  fi
  rm -rf "$TEST_DIR"
}
trap cleanup EXIT
initdb -D "$TEST_DIR/pg" --auth=trust --no-locale >"$TEST_DIR/init.log"
pg_ctl -D "$TEST_DIR/pg" -l "$TEST_DIR/postgres.log" -o "-h 127.0.0.1 -p $PG_PORT -k $TEST_DIR" -w start >/dev/null
createdb -h 127.0.0.1 -p "$PG_PORT" protocol_identity_test
redis-server --bind 127.0.0.1 --port "$REDIS_PORT" --save '' --appendonly no --daemonize yes --pidfile "$TEST_DIR/redis.pid" --logfile "$TEST_DIR/redis.log"
cd "$ROOT_DIR"
PROTOCOL_TEST_DATABASE_URL="postgresql://127.0.0.1:$PG_PORT/protocol_identity_test" \
PROTOCOL_TEST_REDIS_URL="redis://127.0.0.1:$REDIS_PORT/" \
cargo test -p amos-relay --test identity_integration -- --ignored --nocapture
