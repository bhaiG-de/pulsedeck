#!/bin/bash
set -euo pipefail
task_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$task_root"
if [ "$#" -gt 0 ]; then
  echo "Usage: bash tools/test-all.sh (desktop integration: see research/cli-guide.md)" >&2
  exit 2
fi
bash tools/rust-env.sh cargo fmt --all --check
bash tools/rust-env.sh cargo test --workspace -- --skip library::tests::browser_pages_filter_and_sort_in_sql_without_reading_large_analysis_or_cues
# Keep the latency threshold intact; measure without competing regression workers.
bash tools/rust-env.sh cargo test -p pulsedeck library::tests::browser_pages_filter_and_sort_in_sql_without_reading_large_analysis_or_cues -- --test-threads=1
bash tools/rust-env.sh cargo clippy --workspace --all-targets -- -D warnings
npm --prefix frontend test
npm --prefix frontend run build
python3 tools/check-control-coverage.py
python3 tools/validate-dj-skills.py
python3 tools/test-dj-skill-packaging.py
