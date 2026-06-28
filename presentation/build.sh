#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

if ! command -v typst >/dev/null 2>&1; then
  echo "error: typst not found in PATH" >&2
  exit 127
fi

typst compile "$SCRIPT_DIR/rbus.typ" "$SCRIPT_DIR/rbus.pdf"
echo "built $SCRIPT_DIR/rbus.pdf"
