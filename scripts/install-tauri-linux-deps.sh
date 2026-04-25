#!/usr/bin/env bash

set -euo pipefail

mode="ci"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bundle)
      mode="bundle"
      ;;
    --ci|--preflight)
      mode="ci"
      ;;
    *)
      echo "Unknown option: $1" >&2
      echo "Usage: $0 [--ci|--preflight|--bundle]" >&2
      exit 2
      ;;
  esac
  shift
done

sudo apt-get update

base_packages=(
  libgtk-3-dev
  librsvg2-dev
  patchelf
)

bundle_extra_packages=(
  build-essential
  pkg-config
  libssl-dev
  rpm
  xvfb
)

if [[ "${mode}" == "bundle" ]]; then
  sudo apt-get install -y "${base_packages[@]}" "${bundle_extra_packages[@]}"
else
  sudo apt-get install -y "${base_packages[@]}"
fi

# WebKitGTK: Ubuntu 22.04 uses 4.0, newer versions use 4.1
sudo apt-get install -y libwebkit2gtk-4.1-dev || sudo apt-get install -y libwebkit2gtk-4.0-dev

# AppIndicator: some distros use ayatana naming
sudo apt-get install -y libayatana-appindicator3-dev || sudo apt-get install -y libappindicator3-dev

if [[ "${mode}" == "bundle" ]]; then
  # AppImage runtime helpers (best-effort)
  sudo apt-get install -y libfuse2 || sudo apt-get install -y libfuse2t64 || true
fi
