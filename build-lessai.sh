#!/usr/bin/env bash

set -u
set -o pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
LESSAI_ROOT="$SCRIPT_DIR"
BUNDLE_DIR="$LESSAI_ROOT/src-tauri/target/release/bundle"

# Shared Linux helper for dependency checks and repair flow.
# shellcheck source=scripts/lessai-linux-common.sh
source "$LESSAI_ROOT/scripts/lessai-linux-common.sh"

cd "$LESSAI_ROOT" || exit 1

lessai_print_banner "LessAI Packager"

if ! lessai_require_tools; then
  exit 1
fi

lessai_prepare_node_env

if ! lessai_ensure_deps; then
  printf '\n'
  lessai_error "Environment check failed."
  exit 1
fi

lessai_info "Building LessAI (Tauri bundle)..."
lessai_info "This may take a while on first build."
printf '\n'

export RUST_BACKTRACE=1
lessai_run_pnpm exec tauri build
exit_code=$?

printf '\n'
if [ "$exit_code" -ne 0 ]; then
  lessai_error "LessAI build failed with exit code $exit_code."
  lessai_hint "Make sure you are building in the same OS environment that installed node_modules."
  lessai_hint "If you see optional-deps native binding errors, repair install node_modules."
  exit "$exit_code"
fi

lessai_info "Build completed successfully."
printf '\n'
lessai_info "Output directory (default):"
printf '  %s\n\n' "$BUNDLE_DIR"

if [ -d "$BUNDLE_DIR" ]; then
  lessai_info "Bundles:"
  find "$BUNDLE_DIR" -mindepth 1 -maxdepth 1 -printf '  %f\n' 2>/dev/null || ls -1 "$BUNDLE_DIR" | sed 's/^/  /'
else
  lessai_warn "Bundle directory not found. Tauri output path may differ on your system."
fi

exit 0
