#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="${ROOT_DIR}/bin:${PATH}"
export PROTOC="${ROOT_DIR}/bin/protoc"
export PROTOC_INCLUDE="${ROOT_DIR}/bin/include"

CONFIG="${ROOT_DIR}/scenarios/three_region_evidence.json"
WITH_ETCD=0
SEED=42
RUNS=5
OUTPUT_DIR=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --config)
            CONFIG="$2"
            shift 2
            ;;
        --with-etcd)
            WITH_ETCD=1
            shift
            ;;
        --seed)
            SEED="$2"
            shift 2
            ;;
        --runs)
            RUNS="$2"
            shift 2
            ;;
        --output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [--config <path>] [--with-etcd] [--seed <number>] [--runs <number>] [--output-dir <path>]"
            exit 0
            ;;
        *)
            echo "Unknown argument: $1"
            exit 1
            ;;
    esac
done

RUN_TIMESTAMP="$(date -u +%Y%m%d_%H%M%S)"
RUN_ID="run_${RUN_TIMESTAMP}"
if [[ -z "${OUTPUT_DIR}" ]]; then
    OUTPUT_DIR="${ROOT_DIR}/audit/runs/${RUN_ID}"
fi
mkdir -p "${OUTPUT_DIR}"

echo "=========================================================================="
echo "          KAMISH PHASE 4: REPRODUCIBLE SCENARIO EVIDENCE RUNNER          "
echo "=========================================================================="
echo "Run ID:      ${RUN_ID}"
echo "Config:      ${CONFIG}"
echo "With etcd:   ${WITH_ETCD}"
echo "Seed:        ${SEED}"
echo "Runs:        ${RUNS}"
echo "Output Dir:  ${OUTPUT_DIR}"
echo "Git Commit:  $(git rev-parse HEAD)"
echo "Git Branch:  $(git rev-parse --abbrev-ref HEAD)"
echo "--------------------------------------------------------------------------"

ETCD_STARTED=0
cleanup() {
    if [[ "${ETCD_STARTED}" -eq 1 ]]; then
        echo ">> Cleaning up etcd cluster..."
        "${ROOT_DIR}/scripts/stop_etcd_cluster.sh" --clean-data || true
    fi
}
trap cleanup EXIT

if [[ "${WITH_ETCD}" -eq 1 ]]; then
    echo ">> [Phase 4] Step 1: Starting 3-member external etcd 3.7.3 cluster with mTLS..."
    "${ROOT_DIR}/scripts/start_etcd_cluster.sh"
    ETCD_STARTED=1

    echo ">> [Phase 4] Step 2: Executing etcd authority integration test suite..."
    export KAMISH_ETCD_TESTS=1
    cargo test --package kamish-meta --test etcd_authority -- --nocapture --test-threads=1
    echo ">> [Phase 4] etcd authority verified: strictly zero consensus reads on produce path."
fi

echo ">> [Phase 4] Step 3: Executing Scenario Runner (W30, W31, W32 Invariant Pipeline)..."
cargo run -p kamish-checkers --bin scenario_runner -- \
    --config "${CONFIG}" \
    --seed "${SEED}" \
    --runs "${RUNS}" \
    --output-dir "${OUTPUT_DIR}"

# Mirror evidence into audit/phase4_completion
PHASE4_AUDIT_DIR="${ROOT_DIR}/audit/phase4_completion"
mkdir -p "${PHASE4_AUDIT_DIR}"
cp "${OUTPUT_DIR}/results.json" "${PHASE4_AUDIT_DIR}/latest_run.json"
cp "${OUTPUT_DIR}/REPORT.md" "${PHASE4_AUDIT_DIR}/REPORT.md"

echo "=========================================================================="
echo "SUCCESS: Scenario run complete."
echo "Primary Evidence:   ${OUTPUT_DIR}/REPORT.md"
echo "Machine Results:    ${OUTPUT_DIR}/results.json"
echo "Audit Mirrored:     ${PHASE4_AUDIT_DIR}/REPORT.md"
echo "=========================================================================="
