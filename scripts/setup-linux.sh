#!/usr/bin/env bash
# setup-linux.sh — one-time setup for Surface Watch on Linux (Debian/Ubuntu/Fedora/Arch)
#
# Installs the WebKitGTK runtime the window needs and a desktop launcher.
# Checks, without installing, what captures need: Node.js 22.19+ (for REA)
# and a Chrome-family browser. Their versions matter and distributions ship
# old ones, so the script says what to install rather than guessing.
#
# Usage (from the extracted release archive):
#   chmod +x setup-linux.sh && sudo ./setup-linux.sh

set -e

info()  { echo -e "\033[34m[info]\033[0m  $*"; }
ok()    { echo -e "\033[32m[ok]\033[0m    $*"; }
skip()  { echo -e "\033[33m[skip]\033[0m  $*"; }
warn()  { echo -e "\033[33m[warn]\033[0m  $*"; }
err()   { echo -e "\033[31m[error]\033[0m $*"; exit 1; }

if   command -v apt-get &>/dev/null; then DISTRO=debian
elif command -v dnf     &>/dev/null; then DISTRO=fedora
elif command -v pacman  &>/dev/null; then DISTRO=arch
else err "Unsupported distro: install WebKitGTK 4.1 manually (see README)"
fi
info "Detected distro family: $DISTRO"

# ── Runtime dependencies (WebKitGTK + libxdo) ────────────────────────────────
case "$DISTRO" in
  debian)
    PKGS=()
    dpkg -l libwebkit2gtk-4.1-0 &>/dev/null || PKGS+=(libwebkit2gtk-4.1-0)
    dpkg -l libxdo3 &>/dev/null || PKGS+=(libxdo3)
    if [ ${#PKGS[@]} -gt 0 ]; then
      info "Installing runtime libs: ${PKGS[*]}"
      apt-get update -qq
      apt-get install -y "${PKGS[@]}"
      ok "Runtime libs installed"
    else
      skip "Runtime libs already installed"
    fi
    ;;
  fedora)
    rpm -q webkit2gtk4.1 &>/dev/null && skip "WebKitGTK already installed" || { dnf install -y webkit2gtk4.1; ok "WebKitGTK installed"; }
    rpm -q xdotool &>/dev/null || dnf install -y xdotool
    ;;
  arch)
    pacman -Qi webkit2gtk-4.1 &>/dev/null && skip "WebKitGTK already installed" || { pacman -S --noconfirm webkit2gtk-4.1; ok "WebKitGTK installed"; }
    pacman -Qi xdotool &>/dev/null || pacman -S --noconfirm xdotool
    ;;
esac

# ── What captures need: checked, not installed ───────────────────────────────
NODE_OK=false
if command -v node &>/dev/null; then
  NODE_V=$(node --version | tr -d v)
  IFS=. read -r MAJ MIN _ <<< "$NODE_V"
  if { [ "$MAJ" -eq 22 ] && [ "$MIN" -ge 19 ]; } || { [ "$MAJ" -eq 24 ] && [ "$MIN" -ge 11 ]; } || [ "$MAJ" -ge 26 ]; then
    ok "Node.js $NODE_V"
    NODE_OK=true
  else
    warn "Node.js $NODE_V is too old for REA: it needs 22.19+, 24.11+ or 26+"
  fi
else
  warn "Node.js not found"
fi
$NODE_OK || warn "  Install a current Node.js from https://nodejs.org or with nvm (https://github.com/nvm-sh/nvm)."

BROWSER=""
for b in google-chrome google-chrome-stable chromium chromium-browser microsoft-edge; do
  command -v "$b" &>/dev/null && { BROWSER="$b"; break; }
done
if [ -n "$BROWSER" ]; then
  ok "Browser: $BROWSER"
else
  warn "No Chrome-family browser found: install Google Chrome, Chromium or Edge to capture targets."
fi

# ── Desktop launcher ─────────────────────────────────────────────────────────
BINARY_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DESKTOP_FILE="/usr/share/applications/surface-watch.desktop"

ICON_NAME="utilities-terminal"
if [[ -f "$BINARY_DIR/icon.png" ]]; then
  ICON_DIR="/usr/share/icons/hicolor/256x256/apps"
  install -Dm644 "$BINARY_DIR/icon.png" "$ICON_DIR/surface-watch.png"
  command -v gtk-update-icon-cache &>/dev/null && gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true
  ICON_NAME="surface-watch"
fi

if [[ -f "$BINARY_DIR/surface-watch" ]]; then
  cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Name=Surface Watch
Comment=Client-side attack surface monitor for web pentests
Exec=$BINARY_DIR/surface-watch
Icon=$ICON_NAME
Terminal=false
Type=Application
Categories=Development;Security;
StartupWMClass=surface-watch
EOF
  command -v update-desktop-database &>/dev/null && update-desktop-database -q /usr/share/applications || true
  ok "Desktop launcher created"
fi

echo ""
echo "Setup complete. Run ./surface-watch to start the app."
