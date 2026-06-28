#!/usr/bin/env sh
set -eu

cd "$(dirname "$0")/../.."

image="${VYUH_BENCH_IMAGE:-vyuh-bench:local}"

docker build -f vyuh-bench/Dockerfile -t "$image" .
docker run --rm \
  -e BENCH_DURATION="${BENCH_DURATION:-3}" \
  -e BENCH_WARMUP="${BENCH_WARMUP:-1}" \
  -e BENCH_TIERS="${BENCH_TIERS:-t2}" \
  -v "$PWD/vyuh-bench/results:/work/vyuh-bench/results" \
  "$image"
