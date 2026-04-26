#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APPIMAGE_DIR="$ROOT_DIR/src-tauri/target/release/bundle/appimage"
APPDIR="${APPDIR:-$APPIMAGE_DIR/LessAI.AppDir}"

if [[ ! -d "$APPDIR" ]]; then
  echo "[ERROR] AppDir not found: $APPDIR" >&2
  exit 1
fi

OUT_APPIMAGE="${OUT_APPIMAGE:-}"
if [[ -z "$OUT_APPIMAGE" ]]; then
  OUT_APPIMAGE="$(find "$APPIMAGE_DIR" -maxdepth 1 -type f -name '*.AppImage' | head -n 1 || true)"
fi
if [[ -z "$OUT_APPIMAGE" ]]; then
  OUT_APPIMAGE="$APPIMAGE_DIR/LessAI_0.1.0_amd64.AppImage"
fi

PLUGIN_APPIMAGE="${LINUXDEPLOY_PLUGIN_APPIMAGE:-$HOME/.cache/tauri/linuxdeploy-plugin-appimage.AppImage}"
if [[ ! -x "$PLUGIN_APPIMAGE" ]] && [[ -x "$HOME/.cache/tauri/linuxdeploy-plugin-appimage-x86_64.AppImage" ]]; then
  PLUGIN_APPIMAGE="$HOME/.cache/tauri/linuxdeploy-plugin-appimage-x86_64.AppImage"
fi
if [[ ! -x "$PLUGIN_APPIMAGE" ]]; then
  echo "[ERROR] linuxdeploy appimage plugin not found: $PLUGIN_APPIMAGE" >&2
  exit 1
fi

echo "[INFO] AppDir: $APPDIR"
echo "[INFO] Output AppImage: $OUT_APPIMAGE"
echo "[INFO] Plugin: $PLUGIN_APPIMAGE"

# Ensure WebKit subprocess binaries are available in common probe paths.
WEBKIT_SRC=""
for candidate in \
  /usr/lib/x86_64-linux-gnu/webkit2gtk-4.1 \
  /usr/lib/webkit2gtk-4.1 \
  /usr/lib64/webkit2gtk-4.1 \
  /usr/libexec/webkit2gtk-4.1; do
  if [[ -d "$candidate" ]]; then
    WEBKIT_SRC="$candidate"
    break
  fi
done
if [[ -n "$WEBKIT_SRC" ]]; then
  install -d "$APPDIR/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1"
  cp -a "$WEBKIT_SRC/." "$APPDIR/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/"
fi

# Compatibility links for runtimes that search under AppDir/lib*.
install -d "$APPDIR/lib" "$APPDIR/lib64"
ln -sfn ../usr/lib/x86_64-linux-gnu "$APPDIR/lib/x86_64-linux-gnu"
ln -sfn ../usr/lib/webkit2gtk-4.1 "$APPDIR/lib/webkit2gtk-4.1"
ln -sfn ../usr/lib64/webkit2gtk-4.1 "$APPDIR/lib64/webkit2gtk-4.1"

# Prefer host graphics stack to avoid cross-distro EGL/GBM crashes.
find "$APPDIR/usr/lib" \( \
  -name 'libEGL*' -o -name 'libGLES*' -o -name 'libgbm*' \
  -o -name 'libdrm*' -o -name 'libvulkan*' -o -name 'libxatracker*' \
  -o -name 'libgallium*' -o -name 'libMesa*' -o -name 'libnouveau*' \
  -o -name 'libradeon*' -o -name 'libiris*' -o -name 'dri' \
\) -prune -exec rm -rf {} + 2>/dev/null || true
find "$APPDIR/usr/lib" -xtype l -delete 2>/dev/null || true

cat > "$APPDIR/AppRun" <<'EOF'
#!/usr/bin/env bash
set -eo pipefail
HERE="$(dirname "$(readlink -f "$0")")"
cd "$HERE"
export APPDIR="$HERE"

SYSTEM_LIB_PATHS="/usr/lib:/usr/lib64:/usr/lib/x86_64-linux-gnu"
APPDIR_LIB_PATHS="$APPDIR/usr/lib:$APPDIR/usr/lib/x86_64-linux-gnu:$APPDIR/usr/lib64:$APPDIR/lib:$APPDIR/lib/x86_64-linux-gnu:$APPDIR/lib64"
if [[ "${LESSAI_FORCE_BUNDLED_LIBS:-0}" == "1" ]]; then
  export LD_LIBRARY_PATH="$APPDIR_LIB_PATHS:${LD_LIBRARY_PATH:-}:$SYSTEM_LIB_PATHS"
else
  export LD_LIBRARY_PATH="$SYSTEM_LIB_PATHS:${LD_LIBRARY_PATH:-}:$APPDIR_LIB_PATHS"
fi

if [[ -d "$HERE/apprun-hooks" ]]; then
  while IFS= read -r -d '' hook; do
    # shellcheck disable=SC1090
    source "$hook"
  done < <(find "$HERE/apprun-hooks" -maxdepth 1 -type f -print0 | sort -z)
fi

# Single source of truth lives in src-tauri/src/main.rs (apply_linux_graphics_compat_env).
# AppImage defaults to safe mode when unset; users can override via LESSAI_LINUX_GRAPHICS_MODE.
export LESSAI_LINUX_GRAPHICS_MODE="${LESSAI_LINUX_GRAPHICS_MODE:-safe}"

WEBKIT_BASE=""
if [[ "${LESSAI_FORCE_BUNDLED_WEBKIT:-0}" == "1" ]]; then
  WEBKIT_CANDIDATES=(
    "$APPDIR/lib/x86_64-linux-gnu/webkit2gtk-4.1"
    "$APPDIR/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1"
    "$APPDIR/usr/lib/webkit2gtk-4.1"
    "$APPDIR/usr/lib64/webkit2gtk-4.1"
    "/usr/lib/webkit2gtk-4.1"
    "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1"
    "/usr/lib64/webkit2gtk-4.1"
  )
else
  WEBKIT_CANDIDATES=(
    "/usr/lib/webkit2gtk-4.1"
    "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1"
    "/usr/lib64/webkit2gtk-4.1"
    "$APPDIR/lib/x86_64-linux-gnu/webkit2gtk-4.1"
    "$APPDIR/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1"
    "$APPDIR/usr/lib/webkit2gtk-4.1"
    "$APPDIR/usr/lib64/webkit2gtk-4.1"
  )
fi
for candidate in "${WEBKIT_CANDIDATES[@]}"; do
  if [[ -x "$candidate/WebKitNetworkProcess" ]]; then
    WEBKIT_BASE="$candidate"
    break
  fi
done
if [[ -z "$WEBKIT_BASE" ]]; then
  WEBKIT_BASE="/usr/lib/webkit2gtk-4.1"
fi
export WEBKIT_EXEC_PATH="$WEBKIT_BASE"
export WEBKIT_INJECTED_BUNDLE_PATH="$WEBKIT_BASE/injected-bundle"

if [[ "${LESSAI_DEBUG_APPRUN:-}" == "1" ]]; then
  echo "[AppRun] LD_LIBRARY_PATH=$LD_LIBRARY_PATH" >&2
  echo "[AppRun] WEBKIT_EXEC_PATH=$WEBKIT_EXEC_PATH" >&2
  echo "[AppRun] GDK_BACKEND=${GDK_BACKEND:-}" >&2
  echo "[AppRun] EGL_PLATFORM=${EGL_PLATFORM:-}" >&2
fi

exec "$HERE/usr/bin/lessai" "$@"
EOF
chmod +x "$APPDIR/AppRun"

RUNTIME_FILE="$(mktemp)"
OFFSET="$(APPIMAGE_EXTRACT_AND_RUN=1 "$PLUGIN_APPIMAGE" --appimage-offset)"
dd if="$PLUGIN_APPIMAGE" of="$RUNTIME_FILE" bs=1 count="$OFFSET" status=none
chmod +x "$RUNTIME_FILE"

TMP_APPIMAGE_DIR="$(mktemp -d)"
cp "$PLUGIN_APPIMAGE" "$TMP_APPIMAGE_DIR/plugin.AppImage"
(
  cd "$TMP_APPIMAGE_DIR"
  APPIMAGE_EXTRACT_AND_RUN=1 ./plugin.AppImage --appimage-extract >/dev/null
  ARCH=x86_64 "$TMP_APPIMAGE_DIR/squashfs-root/appimagetool-prefix/AppRun" \
    --runtime-file "$RUNTIME_FILE" \
    "$APPDIR" \
    "$OUT_APPIMAGE"
)
chmod +x "$OUT_APPIMAGE"

rm -f "$RUNTIME_FILE"
rm -rf "$TMP_APPIMAGE_DIR"

echo "[INFO] Repacked AppImage: $OUT_APPIMAGE"
