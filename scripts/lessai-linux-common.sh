#!/usr/bin/env bash

set -u
set -o pipefail

LESSAI_ROOT="${LESSAI_ROOT:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)}"
LESSAI_PNPM=()

lessai_print_banner() {
  local title="$1"
  printf '========================================\n'
  printf '%s\n' "$title"
  printf '========================================\n\n'
}

lessai_info() {
  printf '[INFO] %s\n' "$1"
}

lessai_warn() {
  printf '[WARN] %s\n' "$1"
}

lessai_error() {
  printf '[ERROR] %s\n' "$1" >&2
}

lessai_hint() {
  printf '[HINT] %s\n' "$1"
}

lessai_run_pnpm() {
  "${LESSAI_PNPM[@]}" "$@"
}

lessai_require_tools() {
  if command -v pnpm >/dev/null 2>&1; then
    LESSAI_PNPM=(pnpm)
  elif command -v corepack >/dev/null 2>&1; then
    LESSAI_PNPM=(corepack pnpm)
  else
    lessai_error "pnpm was not found."
    lessai_error "Please install Node.js and pnpm first."
    return 1
  fi

  if ! command -v cargo >/dev/null 2>&1; then
    lessai_error "cargo was not found."
    lessai_error "Please install the Rust toolchain first."
    return 1
  fi
}

lessai_prepare_node_env() {
  unset NODE_ENV
  export PNPM_PRODUCTION=false
  export NPM_CONFIG_PRODUCTION=false
  export npm_config_production=false
}

lessai_cleanup_ignored_links() {
  [ -d "$LESSAI_ROOT/node_modules" ] || return 0

  find "$LESSAI_ROOT/node_modules" \
    -mindepth 1 \
    -maxdepth 1 \
    -name '.ignored_*' \
    -exec rm -rf -- {} + 2>/dev/null || true
}

lessai_print_install_hints() {
  lessai_hint "Common Linux causes:"
  lessai_hint "- node_modules was created on another OS (Windows/macOS/WSL) and reused on Linux."
  lessai_hint "- Optional native packages for the current platform were not installed correctly."
  lessai_hint "- Tauri Linux system dependencies are missing on this distro."
  lessai_hint "Recommended fix:"
  lessai_hint "- Delete node_modules and reinstall in the same Linux environment that will run/build the app."
  lessai_hint "- Reinstall with:"
  lessai_hint "    rm -rf node_modules"
  lessai_hint "    pnpm install --prefer-frozen-lockfile --no-prod"
  lessai_hint "- Verify:"
  lessai_hint "    pnpm exec tauri --version"
}

lessai_install_deps() {
  lessai_info "Installing dependencies (including devDependencies)..."
  lessai_info "Command: pnpm install --prefer-frozen-lockfile --no-prod"

  lessai_cleanup_ignored_links
  if ! lessai_run_pnpm install --prefer-frozen-lockfile --no-prod; then
    printf '\n'
    lessai_error "Dependency installation failed."
    lessai_info "Trying to cleanup broken .ignored_* links and retry once..."
    lessai_cleanup_ignored_links
    if ! lessai_run_pnpm install --prefer-frozen-lockfile --no-prod; then
      printf '\n'
      lessai_error "Dependency installation failed again."
      lessai_print_install_hints
      return 1
    fi
  fi

  if [ ! -x "$LESSAI_ROOT/node_modules/.bin/tauri" ]; then
    printf '\n'
    lessai_error "Tauri CLI is still missing after installation."
    lessai_hint "Run: pnpm install --prefer-frozen-lockfile --no-prod"
    lessai_hint "Then verify: pnpm exec tauri --version"
    lessai_print_install_hints
    return 1
  fi

  if ! lessai_run_pnpm exec tauri --version >/dev/null 2>&1; then
    printf '\n'
    lessai_error "Tauri CLI failed to run even though it is installed."
    lessai_hint "This usually means the platform-specific package is missing."
    lessai_offer_repair_install
    return $?
  fi

  return 0
}

lessai_offer_repair_install() {
  printf '\n'
  lessai_info "Repair option: remove node_modules and reinstall from scratch."
  lessai_warn "This will delete the node_modules directory under the project."

  if [ ! -t 0 ]; then
    lessai_error "Repair install requires an interactive terminal."
    lessai_print_install_hints
    return 1
  fi

  local answer
  read -r -p "Proceed with repair install? (y/N) " answer
  case "$answer" in
    y|Y|yes|YES)
      ;;
    *)
      lessai_info "Repair install cancelled."
      return 1
      ;;
  esac

  rm -rf -- "$LESSAI_ROOT/node_modules"
  if [ -e "$LESSAI_ROOT/node_modules" ]; then
    lessai_error "Failed to remove node_modules."
    lessai_hint "Close any editors or shells that may still be using node_modules, then retry."
    lessai_print_install_hints
    return 1
  fi

  lessai_install_deps
}

lessai_ensure_deps() {
  if [ ! -d "$LESSAI_ROOT/node_modules" ]; then
    lessai_install_deps
    return $?
  fi

  if [ ! -x "$LESSAI_ROOT/node_modules/.bin/tauri" ]; then
    lessai_warn "Tauri CLI was not found in node_modules."
    lessai_install_deps
    return $?
  fi

  if lessai_run_pnpm exec tauri --version >/dev/null 2>&1; then
    return 0
  fi

  lessai_warn "Tauri CLI exists but cannot run (native binding may be missing)."
  lessai_hint "This often happens when optionalDependencies were not installed correctly."
  lessai_offer_repair_install
}

lessai_find_listening_pids() {
  local port="$1"

  if command -v lsof >/dev/null 2>&1; then
    lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null | sort -u
    return 0
  fi

  if command -v ss >/dev/null 2>&1; then
    ss -ltnp "( sport = :$port )" 2>/dev/null \
      | awk '{
          if (match($0, /pid=[0-9]+/)) {
            pid = substr($0, RSTART + 4, RLENGTH - 4);
            print pid;
          }
        }' \
      | sort -u
    return 0
  fi

  if command -v fuser >/dev/null 2>&1; then
    fuser -n tcp "$port" 2>/dev/null | tr ' ' '\n' | sed '/^$/d' | sort -u
    return 0
  fi

  return 0
}

lessai_print_pid_details() {
  local pid="$1"
  ps -p "$pid" -o pid=,comm=,args= 2>/dev/null || true
}

lessai_ensure_dev_port_free() {
  local port="$1"
  local -a pids=()
  local pid

  while IFS= read -r pid; do
    [ -n "$pid" ] || continue
    pids+=("$pid")
  done < <(lessai_find_listening_pids "$port")

  if [ "${#pids[@]}" -eq 0 ]; then
    return 0
  fi

  lessai_warn "Port $port is already in use."
  lessai_info "PID(s): ${pids[*]}"
  for pid in "${pids[@]}"; do
    lessai_print_pid_details "$pid"
  done

  if [ ! -t 0 ]; then
    lessai_info "Cancelled. Please close the program that is using port $port and retry."
    return 1
  fi

  local answer
  read -r -p "Terminate these process(es) to continue? (y/N) " answer
  case "$answer" in
    y|Y|yes|YES)
      ;;
    *)
      lessai_info "Cancelled. Please close the program that is using port $port and retry."
      return 1
      ;;
  esac

  for pid in "${pids[@]}"; do
    kill "$pid" 2>/dev/null || true
  done

  sleep 1

  for pid in "${pids[@]}"; do
    if kill -0 "$pid" 2>/dev/null; then
      kill -9 "$pid" 2>/dev/null || true
    fi
  done

  sleep 1

  if lessai_find_listening_pids "$port" | grep -q '[0-9]'; then
    lessai_error "Port $port is still in use after termination attempt."
    return 1
  fi

  return 0
}
