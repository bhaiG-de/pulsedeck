#!/bin/bash
set -euo pipefail
task_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$task_root"
task_audio=()
if [ "${1:-}" = "--audio" ] && [ "$#" = 1 ]; then
  task_audio=(--audio)
elif [ "$#" != 0 ]; then
  echo "Usage: bash tools/test-desktop.sh [--audio]" >&2
  exit 2
fi
task_app="$task_root/target/debug/bundle/macos/PulseDeck.app/Contents/MacOS/pulsedeck"
task_cli="$task_root/target/debug/pulsedeckctl"
if [ ! -x "$task_app" ] || [ ! -x "$task_cli" ]; then
  echo "Build first: npm run package -- --debug; bash tools/rust-env.sh cargo build -p pulsedeck-cli" >&2
  exit 2
fi
task_profile="$(mktemp -d /tmp/pd-cli-e2e.XXXXXX)"
echo "Isolated desktop test artifacts: $task_profile"
task_pid=""
cleanup() {
  if [ -n "$task_pid" ]; then
    "$task_cli" app stop --socket "$task_profile/control.sock" --json >/dev/null 2>&1 || kill "$task_pid" 2>/dev/null || true
    wait "$task_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT
launch() {
  PULSEDECK_ISOLATED=1 PULSEDECK_DATA_DIR="$task_profile/data" PULSEDECK_SOCKET="$task_profile/control.sock" \
    "$task_app" >>"$task_profile/app.log" 2>&1 &
  task_pid=$!
  for task_attempt in {1..100}; do
    if "$task_cli" status --socket "$task_profile/control.sock" --json >"$task_profile/status.json" 2>/dev/null; then
      return
    fi
    if ! kill -0 "$task_pid" 2>/dev/null; then
      cat "$task_profile/app.log" >&2
      return 1
    fi
    sleep 0.1
  done
  echo "Desktop did not expose the control socket within 10s" >&2
  return 1
}
launch
python3 tools/cli_smoke.py --socket "$task_profile/control.sock" --work-dir "$task_profile" "${task_audio[@]}"
cleanup
task_pid=""
launch
python3 tools/cli_smoke.py --socket "$task_profile/control.sock" --work-dir "$task_profile" --restart-check
