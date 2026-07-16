#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND="$ROOT/backend"
FRONTEND="$ROOT/frontend"
API_PORT="${OXIDE_API_PORT:-8000}"
VITE_PORT="${OXIDE_VITE_PORT:-5173}"
CONFIG="${OXIDE_CONFIG:-}"
# prod: build the frontend and serve it from the backend on all interfaces
# (best for accessing from another device on the LAN, e.g. a phone — no Vite
# proxy, so the /api/ws upgrade is direct and there is no proxy EPIPE noise).
# dev (default): Vite dev server on :5173 proxying /api -> backend.
MODE="${OXIDE_MODE:-dev}"

cleanup() {
    echo
    echo "Shutting down dev environment..."
    [[ -n "${BACK_PID:-}" ]] && kill "$BACK_PID" 2>/dev/null || true
    kill_port "$API_PORT"
    [[ "${MODE:-dev}" == "dev" ]] && kill_port "$VITE_PORT"
}
trap cleanup EXIT INT TERM

# Kill any stale dev instances (backend + vite) bound to our ports so a
# previous run that wasn't cleanly shut down doesn't hold the port.
kill_port() {
    local port="$1"
    local pids
    pids="$(lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)"
    if [[ -n "$pids" ]]; then
        echo "==> Killing stale process(es) on :$port"
        kill $pids 2>/dev/null || true
        sleep 1
        pids="$(lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)"
        [[ -n "$pids" ]] && kill -9 $pids 2>/dev/null || true
    fi
}
kill_port "$API_PORT"
[[ "$MODE" == "dev" ]] && kill_port "$VITE_PORT"

echo "==> Building backend (this may take a while on first run)..."
( cd "$BACKEND" && cargo build )

BACK_ARGS=()
if [[ "$MODE" == "prod" ]]; then
    # Bind all interfaces so LAN devices (phone) can reach the single origin
    # that serves both the UI and /api (including the /api/ws socket).
    BACK_ARGS+=(--listen "0.0.0.0:$API_PORT")
    echo "==> Building frontend..."
    ( cd "$FRONTEND" && npm run build )
else
    BACK_ARGS+=(--listen "127.0.0.1:$API_PORT")
fi
if [[ -n "$CONFIG" ]]; then
    BACK_ARGS+=(--config "$CONFIG")
fi

echo "==> Starting backend on :$API_PORT..."
( cd "$BACKEND" && cargo run -- "${BACK_ARGS[@]}" ) &
BACK_PID=$!

if [[ "$MODE" == "prod" ]]; then
    echo "==> Frontend built into frontend/dist and served by the backend."
    echo "    Open http://<this-machine-lan-ip>:$API_PORT on your phone."
    echo "    LAN IP: $(ipconfig getifaddr en0 2>/dev/null || echo '(unknown)')"
    wait "$BACK_PID"
else
    echo "==> Starting frontend dev server (Vite) on :$VITE_PORT..."
    echo "    Frontend proxies /api -> http://127.0.0.1:$API_PORT"
    cd "$FRONTEND"
    npm run dev
    wait "$BACK_PID"
fi
