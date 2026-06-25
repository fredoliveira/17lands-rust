#!/usr/bin/env bash
# Capture the reference Python client's payloads for a given log file (SPEC §12 oracle).
#
# Runs the brew-installed `seventeenlands` against a local mock server in a sandboxed HOME
# (so it never touches the real config or posts to the live API), writing captured POSTs to
# the output JSONL. Outputs may contain account id / screen names from a real log, so write
# them under local/ (gitignored).
#
# Usage: run_oracle.sh <log-file> <output.jsonl> [port]
set -euo pipefail

LOG_FILE="${1:?usage: run_oracle.sh <log-file> <output.jsonl> [port]}"
OUT="${2:?usage: run_oracle.sh <log-file> <output.jsonl> [port]}"
PORT="${3:-8731}"
TOKEN="00000000-0000-4000-8000-000000000000"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SANDBOX="$(mktemp -d)"
trap 'rm -rf "$SANDBOX"' EXIT

# Legacy ini so the Python client's get_config() finds a token instead of prompting on stdin.
printf '[client]\ntoken = %s\n' "$TOKEN" > "$SANDBOX/.mtga_follower.ini"

mkdir -p "$(dirname "$OUT")"

python3 "$SCRIPT_DIR/mock_server.py" "$PORT" "$OUT" &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true; rm -rf "$SANDBOX"' EXIT

# Wait for the server to accept connections.
for _ in $(seq 1 50); do
  if curl -s "http://127.0.0.1:$PORT/api/client/client_version_validation" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

HOME="$SANDBOX" seventeenlands \
  --host "http://127.0.0.1:$PORT" \
  --token "$TOKEN" \
  --log_file "$LOG_FILE" \
  --once

# Give the server a moment to flush the final append, then stop it.
sleep 0.3
kill "$SERVER_PID" 2>/dev/null || true

echo "Captured $(wc -l < "$OUT" | tr -d ' ') submissions to $OUT"
