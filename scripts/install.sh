#!/usr/bin/env bash
# Builds Loom from this checkout and installs it for the current user, with an
# app-launcher entry (freedesktop .desktop file + icons). No root needed.
#
#   scripts/install.sh               build + install (or reinstall)
#   scripts/install.sh --update      git pull loom and openvfs, then rebuild if anything changed
#   scripts/install.sh --nxm         also make Loom the nxm:// handler (Nexus "Mod Manager Download")
#   scripts/install.sh --cli-only    skip the desktop app (no Node.js / WebKitGTK needed)
#   scripts/install.sh --uninstall
#
# Other options: --prefix DIR (default ~/.local), --force, --debug-build,
# --no-integration (just build + copy; no launcher entry, icons or nxm://).
#
# Installs:
#   $PREFIX/bin/loom                                  the CLI (also `loom mcp`)
#   $PREFIX/bin/loom-desktop                          the desktop app
#   $PREFIX/share/applications/dev.loom.app.desktop   launcher entry, handles nxm://
#   $PREFIX/share/icons/hicolor/*/apps/dev.loom.app.png
#   $PREFIX/share/loom/                               install info + a copy of this script
#
# Note: launching games goes through openvfs's Windows backends (WinFsp mount or
# hook); on Linux Loom manages libraries, mods, profiles and load order.
set -euo pipefail

APP_ID=dev.loom.app
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OPENVFS="$(dirname "$REPO")/openvfs"
PREFIX="${HOME}/.local"
UPDATE=0 UNINSTALL=0 FORCE=0 CLI_ONLY=0 NXM=0 DEBUG_BUILD=0 INTEGRATION=1

usage() { sed -n '2,24p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }
while [ $# -gt 0 ]; do
    case "$1" in
        --update) UPDATE=1 ;;
        --uninstall) UNINSTALL=1 ;;
        --force) FORCE=1 ;;
        --cli-only) CLI_ONLY=1 ;;
        --nxm) NXM=1 ;;
        --debug-build) DEBUG_BUILD=1 ;;
        --no-integration) INTEGRATION=0 ;;
        --prefix) PREFIX="${2:?--prefix needs a directory}"; shift ;;
        --prefix=*) PREFIX="${1#--prefix=}" ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
    esac
    shift
done

BIN="$PREFIX/bin"
SHARE="$PREFIX/share"
DATA="$SHARE/loom"
DESKTOP_FILE="$SHARE/applications/$APP_ID.desktop"
INFO="$DATA/install-info"
PROFILE=release; [ "$DEBUG_BUILD" = 1 ] && PROFILE=debug

step() { printf '\033[36m==> %s\033[0m\n' "$*"; }
note() { printf '    %s\n' "$*"; }
die() { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "$1 not found. $2"; }
have() { command -v "$1" >/dev/null 2>&1; }

refresh_desktop_db() {
    have update-desktop-database && update-desktop-database -q "$SHARE/applications" 2>/dev/null || true
    have gtk-update-icon-cache && gtk-update-icon-cache -q -t -f "$SHARE/icons/hicolor" 2>/dev/null || true
    have kbuildsycoca6 && kbuildsycoca6 >/dev/null 2>&1 || { have kbuildsycoca5 && kbuildsycoca5 >/dev/null 2>&1; } || true
}

# ---------------------------------------------------------------- uninstall

if [ "$UNINSTALL" = 1 ]; then
    step "Uninstalling Loom from $PREFIX"
    if have xdg-mime && [ "$(xdg-mime query default x-scheme-handler/nxm 2>/dev/null)" = "$APP_ID.desktop" ]; then
        previous="$(cat "$DATA/nxm-previous" 2>/dev/null || true)"
        if [ -n "$previous" ]; then
            xdg-mime default "$previous" x-scheme-handler/nxm && note "nxm:// handed back to $previous"
        else
            # No earlier handler: drop our line from the user's mimeapps.list.
            for f in "${XDG_CONFIG_HOME:-$HOME/.config}/mimeapps.list" "$SHARE/applications/mimeapps.list"; do
                [ -f "$f" ] && sed -i "\#^x-scheme-handler/nxm=$APP_ID.desktop;\{0,1\}\$#d" "$f"
            done
            note 'nxm:// handler removed'
        fi
    fi
    rm -f "$BIN/loom" "$BIN/loom-desktop" "$DESKTOP_FILE"
    for size in 32x32 128x128 256x256 512x512; do rm -f "$SHARE/icons/hicolor/$size/apps/$APP_ID.png"; done
    rm -rf "$DATA"
    refresh_desktop_db
    note "your mod library (\${XDG_DATA_HOME:-~/.local/share}/Loom) was left alone"
    printf '\033[32mLoom uninstalled.\033[0m\n'
    exit 0
fi

# ---------------------------------------------------------------- sources

need git 'Install git with your package manager.'
need cargo 'Install Rust: https://rustup.rs'
if [ "$CLI_ONLY" = 0 ]; then
    need npm 'Install Node.js (https://nodejs.org or your package manager), or pass --cli-only.'
    if have pkg-config; then
        missing=()
        for lib in webkit2gtk-4.1 javascriptcoregtk-4.1 libsoup-3.0 gtk+-3.0; do
            pkg-config --exists "$lib" || missing+=("$lib")
        done
        if [ ${#missing[@]} -gt 0 ]; then
            die "the desktop app needs WebKitGTK development files (missing: ${missing[*]}).
    Debian/Ubuntu: sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev build-essential pkg-config
    Fedora:        sudo dnf install webkit2gtk4.1-devel gtk3-devel librsvg2-devel
    Arch:          sudo pacman -S webkit2gtk-4.1 gtk3 librsvg base-devel
    Or pass --cli-only. Full list: https://v2.tauri.app/start/prerequisites/#linux"
        fi
    else
        note 'pkg-config not found; skipping the WebKitGTK check'
    fi
fi

if [ ! -f "$OPENVFS/Cargo.toml" ]; then
    step "Cloning openvfs next to loom ($OPENVFS)"
    git clone https://github.com/enrell/openvfs "$OPENVFS"
fi

if [ "$UPDATE" = 1 ]; then
    for dir in "$REPO" "$OPENVFS"; do
        step "Updating $(basename "$dir")"
        git -C "$dir" pull --ff-only
    done
fi

commit() {
    local c
    c="$(git -C "$1" rev-parse HEAD 2>/dev/null)" || return 0
    if [ -n "$(git -C "$1" status --porcelain 2>/dev/null)" ]; then echo "$c-dirty"; else echo "$c"; fi
}
LOOM_COMMIT="$(commit "$REPO")"
OPENVFS_COMMIT="$(commit "$OPENVFS")"

if [ "$UPDATE" = 1 ] && [ "$FORCE" = 0 ] && [ -f "$INFO" ] && [[ "$LOOM_COMMIT" != *-dirty ]] &&
    grep -qx "loom=$LOOM_COMMIT" "$INFO" && grep -qx "openvfs=$OPENVFS_COMMIT" "$INFO" &&
    { [ "$CLI_ONLY" = 1 ] || [ -x "$BIN/loom-desktop" ]; }; then
    printf '\033[32mLoom is up to date (%s).\033[0m\n' "${LOOM_COMMIT:0:7}"
    exit 0
fi

# ---------------------------------------------------------------- build

cargo_profile=(--release); [ "$DEBUG_BUILD" = 1 ] && cargo_profile=()
target_dir() { if [ -n "${CARGO_TARGET_DIR:-}" ]; then echo "$CARGO_TARGET_DIR/$PROFILE"; else echo "$1/target/$PROFILE"; fi; }

step 'Building loom (CLI)'
(cd "$REPO" && cargo build -p loom-cli "${cargo_profile[@]}")

if [ "$CLI_ONLY" = 0 ]; then
    desktop="$REPO/apps/desktop"
    if [ ! -f "$desktop/node_modules/.package-lock.json" ] ||
        [ "$desktop/package-lock.json" -nt "$desktop/node_modules/.package-lock.json" ]; then
        step 'Installing desktop app dependencies (npm ci)'
        (cd "$desktop" && npm ci)
    fi
    step 'Building desktop app (tauri build --no-bundle)'
    tauri_args=(tauri build --no-bundle); [ "$DEBUG_BUILD" = 1 ] && tauri_args+=(--debug)
    (cd "$desktop" && npx "${tauri_args[@]}")
fi

# ---------------------------------------------------------------- install

step "Installing to $PREFIX"
mkdir -p "$BIN" "$DATA"

# Copy then rename: a running Loom keeps its old file, the next start gets the new one.
put() {
    [ -f "$1" ] || die "build output missing: $1"
    install -m 755 "$1" "$2.new" && mv -f "$2.new" "$2"
}
put "$(target_dir "$REPO")/loom" "$BIN/loom"
if [ "$CLI_ONLY" = 0 ]; then
    put "$(target_dir "$REPO/apps/desktop/src-tauri")/loom-desktop" "$BIN/loom-desktop"
fi
[ "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)" = "$DATA" ] || install -m 755 "${BASH_SOURCE[0]}" "$DATA/install.sh"

VERSION="$(sed -n 's/^ *"version": *"\([^"]*\)".*/\1/p' "$REPO/apps/desktop/src-tauri/tauri.conf.json" | head -n1)"
cat >"$INFO" <<EOF
version=$VERSION
loom=$LOOM_COMMIT
openvfs=$OPENVFS_COMMIT
profile=$PROFILE
source=$REPO
installed=$(date -u +%Y-%m-%dT%H:%M:%SZ)
EOF

# ---------------------------------------------------------------- integration

if [ "$INTEGRATION" = 1 ] && [ -x "$BIN/loom-desktop" ]; then
    step 'Adding Loom to the app launcher'
    icons="$REPO/apps/desktop/src-tauri/icons"
    for pair in 32x32:32x32.png 128x128:128x128.png 256x256:128x128@2x.png 512x512:icon.png; do
        size="${pair%%:*}" file="${pair#*:}"
        install -D -m 644 "$icons/$file" "$SHARE/icons/hicolor/$size/apps/$APP_ID.png"
    done
    mkdir -p "$SHARE/applications"
    cat >"$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=Loom
GenericName=Mod Manager
Comment=Mod manager that never touches your game folder
Exec="$BIN/loom-desktop" %u
TryExec=$BIN/loom-desktop
Icon=$APP_ID
Terminal=false
Categories=Game;Utility;
Keywords=mods;modding;nexus;skyrim;fallout;
MimeType=x-scheme-handler/nxm;
StartupWMClass=loom-desktop
EOF
    refresh_desktop_db
    note "launcher entry: $DESKTOP_FILE"

    if [ "$NXM" = 1 ]; then
        if have xdg-mime; then
            previous="$(xdg-mime query default x-scheme-handler/nxm 2>/dev/null || true)"
            if [ -n "$previous" ] && [ "$previous" != "$APP_ID.desktop" ]; then echo "$previous" >"$DATA/nxm-previous"; fi
            xdg-mime default "$APP_ID.desktop" x-scheme-handler/nxm
            note 'nxm:// links now open in Loom'
        else
            note 'skipped --nxm: xdg-mime not found (install xdg-utils)'
        fi
    elif have xdg-mime && [ -z "$(xdg-mime query default x-scheme-handler/nxm 2>/dev/null || true)" ]; then
        note 'tip: --nxm makes Loom open Nexus "Mod Manager Download" links'
    fi
elif [ "$INTEGRATION" = 1 ] && [ "$NXM" = 1 ]; then
    note 'skipped --nxm: the desktop app is not installed'
fi

printf '\033[32mLoom %s (%s) installed.\033[0m\n' "$VERSION" "${LOOM_COMMIT:0:7}"
[ "$CLI_ONLY" = 0 ] && note 'Start it from your app launcher ("Loom") or run loom-desktop'
note "CLI: loom --help    Update: scripts/install.sh --update    Remove: scripts/install.sh --uninstall"
case ":$PATH:" in
    *":$BIN:"*) ;;
    *) note "$BIN is not on your PATH; add it in your shell profile: export PATH=\"$BIN:\$PATH\"" ;;
esac
