#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
SOURCE="$SCRIPT_DIR/vibeshell"
INSTALL_DIR=${VIBESHELL_INSTALL_DIR:-"$HOME/.local/bin"}
DESTINATION="$INSTALL_DIR/vibeshell"

if [ ! -f "$SOURCE" ]; then
  echo "VibeShell CLI binary not found next to this installer: $SOURCE" >&2
  exit 1
fi

mkdir -p "$INSTALL_DIR"
TEMPORARY="$INSTALL_DIR/.vibeshell-install-$$"
trap 'rm -f "$TEMPORARY"' EXIT INT TERM
cp "$SOURCE" "$TEMPORARY"
chmod 755 "$TEMPORARY"
mv -f "$TEMPORARY" "$DESTINATION"
trap - EXIT INT TERM

# The first native invocation installs the bundled Skill into every detected
# coding-agent directory plus the universal ~/.agents/skills location.
"$DESTINATION" version >/dev/null

echo "Installed native VibeShell CLI: $DESTINATION"
case ":${PATH:-}:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo "Add this directory to PATH before opening a new coding-agent shell:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac

echo "Run: vibeshell import auto --dry-run"
