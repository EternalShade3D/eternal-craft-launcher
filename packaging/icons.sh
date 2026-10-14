#!/usr/bin/env bash
# Regenerate every app and tray icon from the launcher's own mark in docs/brand/, rendered from the
# vector the fork's author drew in Blender (Mayor Regular's E, #7149f9).
#
# - App icon (Dock, taskbar, Start menu, Linux menus, installers): the mark.
# - Tray: the black mark as the macOS menu-bar template; the coloured mark on Windows, Linux and BSD.
#
# Needs: resvg (brew install resvg / cargo install resvg). The .ico and .icns are packed by
# `cargo xtask ico` / `cargo xtask icns`, so this runs on any OS. The outputs are committed, so
# packaging (and the Rust build, which include_bytes!s some of them) never needs these tools.
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/assets/app-icon"
BRAND="$ROOT/docs/brand"
SVG="$BRAND/eternal-mark.svg"
ID="ai.storyteller.artcraft-launcher"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

command -v resvg >/dev/null || { echo "error: resvg not found (brew install resvg)" >&2; exit 1; }

# The mark's artwork is untouched. macOS icons get a little more transparent margin (Apple's
# 824/1024 icon grid) by widening the view box around it; everything else uses the file as is.
BOX='viewBox="0 0 2.389 2.389"'
grep -q "$BOX" "$SVG" || { echo "error: expected $BOX in $SVG" >&2; exit 1; }
MAC="$TMP/mac.svg"
sed "s/$BOX/viewBox=\"0.131 0.131 2.651 2.651\"/" "$SVG" >"$MAC"
FLAT="$SVG"

render() { resvg -w "$2" -h "$2" "$1" "$3" </dev/null; }

render "$MAC" 1024 "$DIR/eternal-craft-launcher-1024.png"

# Linux hicolor theme.
for s in 16 24 32 48 64 128 256 512; do
  mkdir -p "$DIR/hicolor/${s}x${s}/apps"
  render "$FLAT" "$s" "$DIR/hicolor/${s}x${s}/apps/$ID.png"
done
mkdir -p "$DIR/hicolor/scalable/apps"
cp "$SVG" "$DIR/hicolor/scalable/apps/$ID.svg"
cp "$SVG" "$DIR/eternal-craft-launcher.svg"

# Windows .ico.
ICO_PNGS=()
for s in 16 20 24 32 40 48 64 128 256; do
  render "$FLAT" "$s" "$TMP/ico-$s.png"
  ICO_PNGS+=("$TMP/ico-$s.png")
done
(cd "$ROOT" && cargo run -q -p xtask -- ico "$DIR/eternal-craft-launcher.ico" "${ICO_PNGS[@]}")

# macOS .icns.
ICNS_PNGS=()
for s in 16 32 64 128 256 512 1024; do
  render "$MAC" "$s" "$TMP/icns-$s.png"
  ICNS_PNGS+=("$TMP/icns-$s.png")
done
(cd "$ROOT" && cargo run -q -p xtask -- icns "$DIR/eternal-craft-launcher.icns" "${ICNS_PNGS[@]}")

# Tray icons: the black mark is the macOS menu-bar template (44 px = 22 pt @2x; macOS tints it);
# the coloured one is for the Windows notification area and Linux/BSD status notifiers.
render "$BRAND/eternal-mark-black.svg" 44 "$BRAND/eternal-mark-black-44.png"
render "$BRAND/eternal-mark.svg" 64 "$BRAND/eternal-mark-64.png"
echo "icons written to $DIR"
