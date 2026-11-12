#!/usr/bin/env sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
OUT=${1:-"$ROOT/artifacts/demo"}
if [ -e "$OUT" ]; then
  echo "Refusing to overwrite existing demo directory: $OUT" >&2
  exit 2
fi
mkdir -p "$OUT"
cargo run --manifest-path "$ROOT/Cargo.toml" -- fixtures generate --output-dir "$OUT/fixtures" --test-signing
cargo run --manifest-path "$ROOT/Cargo.toml" -- inspect "$OUT/fixtures/sample.png"
cargo run --manifest-path "$ROOT/Cargo.toml" -- run --input "$OUT/fixtures/sample.png" \
  --pipeline "$ROOT/examples/pipelines/full-chain.yml" --output-dir "$OUT/run" \
  --mode compare --test-signing --policy "$ROOT/examples/policies/strict.yml"
cargo run --manifest-path "$ROOT/Cargo.toml" -- verify --run "$OUT/run/run.json" \
  --policy "$ROOT/examples/policies/strict.yml"
cargo run --manifest-path "$ROOT/Cargo.toml" -- report --run "$OUT/run/run.json" \
  --format html --output "$OUT/report.html"
cargo run --manifest-path "$ROOT/Cargo.toml" -- report --run "$OUT/run/run.json" \
  --format dot --output "$OUT/chain.dot"
cargo run --manifest-path "$ROOT/Cargo.toml" -- report --run "$OUT/run/run.json" \
  --format json --output "$OUT/report.json"
printf '\nDemo complete. Open %s to compare raw processing with signed reissue.\n' "$OUT/report.html"
printf 'Signed outputs use an ephemeral, untrusted test identity generated in memory by the C2PA SDK.\n'
