#!/usr/bin/env bash
# Sets up Tududu for development on macOS and runs it.
#
# Installs any missing build tools (Xcode Command Line Tools, Homebrew, CMake,
# Rust, Bun), clones the repository if this isn't already a checkout, installs
# the JavaScript dependencies, downloads the voice-detection model the build
# needs, then starts Tududu. Safe to run again: anything installed is skipped.
#
# Usage, from inside a clone:
#   ./scripts/setup-macos.sh            # set up and start the app
#   ./scripts/setup-macos.sh --build    # set up and build a .dmg
#   ./scripts/setup-macos.sh --no-run   # set up only
#
# Without cloning first:
#   curl -fsSL https://raw.githubusercontent.com/TaniKroos/tududu/main/scripts/setup-macos.sh | bash

set -euo pipefail

REPO_URL="https://github.com/TaniKroos/tududu.git"
VAD_URL="https://huggingface.co/handy-computer/silero-vad-v6.2-gguf/resolve/main/silero-vad-v6.2-F32.gguf"
MODE="run"
DIR="tududu"

for arg in "$@"; do
  case "$arg" in
    --build) MODE="build" ;;
    --no-run) MODE="none" ;;
    --dir=*) DIR="${arg#--dir=}" ;;
    -h | --help)
      sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "Unknown option: $arg (try --help)" >&2
      exit 2
      ;;
  esac
done

step() { printf '\n\033[36m==> %s\033[0m\n' "$1"; }
ok() { printf '    \033[32m%s\033[0m\n' "$1"; }
note() { printf '    \033[33m%s\033[0m\n' "$1"; }
fail() {
  printf '\n\033[31mSetup stopped: %s\033[0m\n' "$1" >&2
  exit 1
}
has() { command -v "$1" >/dev/null 2>&1; }

printf '\033[36mTududu setup for macOS\033[0m\n'
[ "$(uname -s)" = "Darwin" ] || fail "this script is for macOS. On Windows, use scripts/setup-windows.ps1."

# --- Tools -----------------------------------------------------------------
step "Checking build tools"

if xcode-select -p >/dev/null 2>&1; then
  ok "Xcode Command Line Tools found"
else
  note "Installing Xcode Command Line Tools: click Install in the window that opens."
  xcode-select --install || true
  until xcode-select -p >/dev/null 2>&1; do sleep 5; done
  ok "Xcode Command Line Tools installed"
fi

# Homebrew lives in /opt/homebrew on Apple Silicon and /usr/local on Intel.
for prefix in /opt/homebrew /usr/local; do
  if [ -x "$prefix/bin/brew" ]; then eval "$("$prefix/bin/brew" shellenv)"; fi
done
if has brew; then
  ok "Homebrew found"
else
  note "Installing Homebrew (it will ask for your Mac password)..."
  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)" </dev/tty
  for prefix in /opt/homebrew /usr/local; do
    if [ -x "$prefix/bin/brew" ]; then eval "$("$prefix/bin/brew" shellenv)"; fi
  done
  has brew || fail "Homebrew didn't install. See https://brew.sh, then run this script again."
fi

if has cmake; then ok "CMake found"; else
  note "Installing CMake..."
  brew install cmake
fi

export PATH="$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"
if has rustup; then ok "Rust found"; else
  note "Installing Rust..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
rustup toolchain install stable --profile minimal >/dev/null
rustup default stable >/dev/null

if has bun; then ok "Bun found"; else
  note "Installing Bun..."
  curl -fsSL https://bun.sh/install | bash
fi

for tool in git cmake cargo bun; do
  has "$tool" || fail "$tool isn't available after installing. Open a new Terminal and run this script again."
done

# --- Source ----------------------------------------------------------------
step "Getting the Tududu source"
is_checkout() { [ -f "$1/package.json" ] && [ -f "$1/src-tauri/tauri.conf.json" ]; }
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || true)"
if is_checkout "."; then
  ok "Using the checkout in $(pwd)"
elif [ -n "$script_dir" ] && is_checkout "$script_dir/.."; then
  cd "$script_dir/.."
  ok "Using the checkout in $(pwd)"
else
  [ -e "$DIR" ] && fail "'$DIR' already exists but isn't a Tududu checkout. Pass --dir=<folder>."
  git clone "$REPO_URL" "$DIR"
  cd "$DIR"
  ok "Cloned into $(pwd)"
fi

# --- Dependencies ----------------------------------------------------------
step "Installing JavaScript dependencies"
bun install

step "Downloading the voice-detection model"
vad="src-tauri/resources/models/silero-vad-v6.2-F32.gguf"
if [ -f "$vad" ]; then
  ok "Already downloaded"
else
  mkdir -p "$(dirname "$vad")"
  curl -fL --progress-bar -o "$vad.part" "$VAD_URL"
  mv "$vad.part" "$vad"
  ok "Saved to $vad"
fi

# --- Run -------------------------------------------------------------------
# CMake 4 refuses some dependencies' old minimum-version declarations without this.
export CMAKE_POLICY_VERSION_MINIMUM=3.5

case "$MODE" in
  none)
    step "Setup complete"
    echo "    Start Tududu with:"
    echo "      CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev"
    ;;
  build)
    step "Building the app (the first build takes 10-25 minutes)"
    bun run tauri build
    step "Done. The .dmg is in src-tauri/target/release/bundle/dmg/"
    ls src-tauri/target/release/bundle/dmg/*.dmg 2>/dev/null || true
    ;;
  run)
    step "Starting Tududu (the first build takes 10-25 minutes; keep this window open)"
    echo "    When the window opens: allow Microphone and Accessibility, then"
    echo "    Models -> download and select 'Canary 1B v2', then"
    echo "    General -> Translation -> pick a language. Hold Option+T and speak English."
    exec bun run tauri dev
    ;;
esac
