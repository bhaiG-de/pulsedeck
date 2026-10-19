#!/usr/bin/env bash
# Exercise the exact vendored native DSP with C++ new/delete instrumentation.
# This complements the Rust callback allocator audit without overriding any
# allocator inside the desktop app. Only an isolated generated-audio binary runs.
set -euo pipefail
task_root="$(cd "$(dirname "$0")/.." && pwd)"
task_probe_dir="$(mktemp -d "${TMPDIR:-/tmp}/pulsedeck-native-allocations.XXXXXX")"
trap 'rm -rf "$task_probe_dir"' EXIT
"${CXX:-c++}" -O3 -std=c++17 \
  -I "$task_root/crates/deck-engine/vendor" \
  -I "$task_root/crates/deck-engine/vendor/signalsmith-stretch" \
  "$task_root/tools/audio/native-allocation-probe.cpp" -o "$task_probe_dir/probe"
"$task_probe_dir/probe"
