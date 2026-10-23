#!/usr/bin/env bash
# Crate boundaries (docs/CODE-ARCHITECTURE.md §2): the core never links a terminal library, and
# heavy dependencies stay in their owning crate. Exits non-zero and names the offending crate.
set -euo pipefail
cd "$(dirname "$0")/.."

members=$(cargo metadata --format-version 1 --no-deps)

forbid() { # forbid <package> <crate>...
  local pkg=$1; shift
  grep -q "\"name\":\"$pkg\"" <<<"$members" || return 0
  local tree
  tree=$(cargo tree -p "$pkg" -e normal --prefix none --format '{p}' | awk '{print $1}' | sort -u)
  for bad in "$@"; do
    if grep -qx "$bad" <<<"$tree"; then
      echo "boundary violation: $pkg depends on $bad" >&2
      exit 1
    fi
  done
}

forbid pawsman-error serde tokio
forbid pawsman-model tokio reqwest ratatui crossterm rhai
forbid pawsman-script tokio reqwest ratatui crossterm
forbid pawsman-import tokio reqwest ratatui crossterm rhai
forbid pawsman-core ratatui crossterm tachyonfx ratatui-image
forbid pawsman-net ratatui crossterm rhai
echo "crate boundaries ok"
