#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND="$ROOT/backend"
FRONTEND="$ROOT/frontend"
API_PORT="${OXIDE_API_PORT:-8000}"
CONFIG="${OXIDE_CONFIG:-}"

cleanup() {
    echo
    echo "Shutting down dev environment..."
    [[ -n "${BACK_PID:-}" ]] && kill "$BACK_PID" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

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
