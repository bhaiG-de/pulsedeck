#!/bin/bash
set -euo pipefail
task_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$task_root"
task_name="workflow-$(date +%s)-$$"
task_started=0
cleanup() {
  if [ "$task_started" = 1 ]; then
    python3 tools/skill-trials/profile.py stop "$task_name" >/dev/null
  fi
}
trap cleanup EXIT
python3 tools/skill-trials/profile.py start "$task_name" --audio
task_started=1
python3 tools/skill-trials/workflow.py "artifacts/skill-trials/$task_name/profile.json"
