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
  tree=$(cargo tree -p "$pkg" -e normal --all-features --target all --prefix none --format '{p}' | awk '{print $1}' | sort -u)
  for bad in "$@"; do
    if grep -qx "$bad" <<<"$tree"; then
      echo "boundary violation: $pkg depends on $bad" >&2
      exit 1
    fi
  done
}

forbid pawsman-error serde tokio
ui=(ratatui ratatui-core ratatui-widgets crossterm tachyonfx ratatui-image)
forbid pawsman-model tokio hyper rustls reqwest rhai notify "${ui[@]}"
forbid pawsman-script tokio reqwest "${ui[@]}"
forbid pawsman-import tokio reqwest rhai "${ui[@]}"
# core reaches tokio only through pawsman-net (ARCHITECTURE §1); deny.toml wrappers keep it out of core itself.
forbid pawsman-core reqwest "${ui[@]}"
forbid pawsman-net rhai notify "${ui[@]}"
# The TUI and the CLI reach tokio through core and net; deny.toml wrappers keep it out of their code.
forbid pawsman-tui reqwest rhai
forbid pawsman-cli reqwest
echo "crate boundaries ok"
