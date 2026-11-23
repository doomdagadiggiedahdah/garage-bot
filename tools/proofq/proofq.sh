#!/bin/sh
# proofq launcher: runs ~/proofq/proofq.py with uv (stdlib only, no dependencies).
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
exec uv run --quiet --no-project --script "$HOME/proofq/proofq.py" "$@"
