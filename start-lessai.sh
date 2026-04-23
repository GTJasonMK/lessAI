#!/usr/bin/env bash

set -u
set -o pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
LESSAI_ROOT="$SCRIPT_DIR"

# Shared Linux helper for dependency checks and repair flow.
# shellcheck source=scripts/lessai-linux-common.sh
source "$LESSAI_ROOT/scripts/lessai-linux-common.sh"

cd "$LESSAI_ROOT" || exit 1

lessai_print_banner "LessAI Launcher"

if ! lessai_require_tools; then
  exit 1
fi

lessai_prepare_node_env

if ! lessai_ensure_deps; then
  printf '\n'
  lessai_error "Environment check failed."
  exit 1
fi

lessai_info "Starting LessAI in dev mode..."
lessai_info "First launch may take a while because Rust will compile."
lessai_info "Close the app window or this terminal to stop it."
printf '\n'

if ! lessai_ensure_dev_port_free 1420; then
  printf '\n'
  lessai_error "Dev server port 1420 is not available."
  lessai_hint "Close the program using port 1420, or allow this script to terminate it."
  exit 1
fi

lessai_run_pnpm exec tauri dev
exit_code=$?

printf '\n'
if [ "$exit_code" -ne 0 ]; then
  lessai_error "LessAI exited with code $exit_code."
else
  lessai_info "LessAI exited normally."
fi

exit "$exit_code"
