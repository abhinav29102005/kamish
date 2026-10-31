#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="${ROOT_DIR}/bin:${PATH}"
export PROTOC="${ROOT_DIR}/bin/protoc"
export PROTOC_INCLUDE="${ROOT_DIR}/bin/include"

echo "=== [run_etcd_tests] 1. Preparing etcd binaries & TLS certificates ==="
"${ROOT_DIR}/scripts/download_etcd.sh"
"${ROOT_DIR}/scripts/generate_test_certs.sh"

echo "=== [run_etcd_tests] 2. Starting 3-member etcd cluster ==="
"${ROOT_DIR}/scripts/start_etcd_cluster.sh"

# Trap cleanup so cluster is always stopped even on test failure
cleanup() {
    echo "=== [run_etcd_tests] 4. Stopping etcd cluster & cleaning state ==="
    "${ROOT_DIR}/scripts/stop_etcd_cluster.sh" --clean-data
}
trap cleanup EXIT

echo "=== [run_etcd_tests] 3. Running etcd integration test target ==="
export KAMISH_ETCD_TESTS=1
cargo test --package kamish-meta --test etcd_authority -- --nocapture --test-threads=1

echo "=== [run_etcd_tests] SUCCESS: All etcd authority integration tests passed! ==="
