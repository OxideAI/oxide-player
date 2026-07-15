#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND="$ROOT/backend"
FRONTEND="$ROOT/frontend"
API_PORT="${OXIDE_API_PORT:-8000}"
VITE_PORT="${OXIDE_VITE_PORT:-5173}"
CONFIG="${OXIDE_CONFIG:-}"

cleanup() {
    echo
    echo "Shutting down dev environment..."
    [[ -n "${BACK_PID:-}" ]] && kill "$BACK_PID" 2>/dev/null || true
    kill_port "$API_PORT"
    kill_port "$VITE_PORT"
}
trap cleanup EXIT INT TERM

# Kill any stale dev instances (backend + vite) bound to our ports so a
# previous `dev.sh` run that wasn't cleanly shut down doesn't hold the port
# and force the new frontend into ECONNREFUSED proxy errors.
kill_port() {
    local port="$1"
    local pids
    pids="$(lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)"
    if [[ -n "$pids" ]]; then
        echo "==> Killing stale process(es) on :$port"
        kill $pids 2>/dev/null || true
        sleep 1
        # Force-kill anything still holding the port.
        pids="$(lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)"
        [[ -n "$pids" ]] && kill -9 $pids 2>/dev/null || true
    fi
}
kill_port "$API_PORT"
kill_port "$VITE_PORT"

echo "==> Building backend (this may take a while on first run)..."
( cd "$BACKEND" && cargo build )

BACK_ARGS=(--listen "127.0.0.1:$API_PORT")
if [[ -n "$CONFIG" ]]; then
    BACK_ARGS+=(--config "$CONFIG")
fi

echo "==> Starting backend on :$API_PORT..."
( cd "$BACKEND" && cargo run -- "${BACK_ARGS[@]}" ) &
BACK_PID=$!

echo "==> Starting frontend dev server (Vite)..."
echo "    Frontend proxies /api -> http://127.0.0.1:$API_PORT"
cd "$FRONTEND"
npm run dev

wait "$BACK_PID"
