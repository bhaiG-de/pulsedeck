#!/bin/bash
set -euo pipefail
task_root="$(cd "$(dirname "$0")/.." && pwd)"
if [ -d "$task_root/.toolchain/cargo/bin" ]; then
  export CARGO_HOME="$task_root/.toolchain/cargo"
  export RUSTUP_HOME="$task_root/.toolchain/rustup"
  export PATH="$CARGO_HOME/bin:$PATH"
fi
exec "$@"
