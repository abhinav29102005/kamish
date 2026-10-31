#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${ROOT_DIR}/bin"
ETCD_BIN="${BIN_DIR}/etcd"
ETCDCTL_BIN="${BIN_DIR}/etcdctl"
CERTS_DIR="${ROOT_DIR}/certs"
DATA_DIR="${ROOT_DIR}/data/etcd"
PID_DIR="${DATA_DIR}/pids"

# 1. Ensure binary and certs exist
if [[ ! -x "${ETCD_BIN}" ]]; then
    "${ROOT_DIR}/scripts/download_etcd.sh"
fi

if [[ ! -f "${CERTS_DIR}/ca.crt" ]]; then
    "${ROOT_DIR}/scripts/generate_test_certs.sh"
fi

# 2. Stop any existing cluster
"${ROOT_DIR}/scripts/stop_etcd_cluster.sh"

mkdir -p "${PID_DIR}"

INITIAL_CLUSTER="us-east=https://127.0.0.1:2380,eu-central=https://127.0.0.1:2382,ap-south=https://127.0.0.1:2384"
TOKEN="kamish-authority-test"

start_member() {
    local NAME=$1
    local CLIENT_PORT=$2
    local PEER_PORT=$3
    local MEMBER_DIR="${DATA_DIR}/${NAME}.etcd"
    mkdir -p "${MEMBER_DIR}"

    local CLUSTER_STATE="new"
    if [[ -d "${MEMBER_DIR}/member" ]]; then
        CLUSTER_STATE="existing"
    fi

    echo "[start_etcd_cluster] Launching member ${NAME} (state: ${CLUSTER_STATE}, client: ${CLIENT_PORT}, peer: ${PEER_PORT})..."
    nohup "${ETCD_BIN}" \
        --name "${NAME}" \
        --data-dir "${MEMBER_DIR}" \
        --listen-client-urls "https://127.0.0.1:${CLIENT_PORT}" \
        --advertise-client-urls "https://127.0.0.1:${CLIENT_PORT}" \
        --listen-peer-urls "https://127.0.0.1:${PEER_PORT}" \
        --initial-advertise-peer-urls "https://127.0.0.1:${PEER_PORT}" \
        --initial-cluster "${INITIAL_CLUSTER}" \
        --initial-cluster-token "${TOKEN}" \
        --initial-cluster-state "${CLUSTER_STATE}" \
        --client-cert-auth \
        --trusted-ca-file "${CERTS_DIR}/ca.crt" \
        --cert-file "${CERTS_DIR}/${NAME}.crt" \
        --key-file "${CERTS_DIR}/${NAME}.key" \
        --peer-client-cert-auth \
        --peer-trusted-ca-file "${CERTS_DIR}/ca.crt" \
        --peer-cert-file "${CERTS_DIR}/${NAME}.crt" \
        --peer-key-file "${CERTS_DIR}/${NAME}.key" \
        > "${DATA_DIR}/${NAME}.log" 2>&1 < /dev/null &

    local PID=$!
    echo "${PID}" > "${PID_DIR}/${NAME}.pid"
}

start_member "us-east" 2379 2380
start_member "eu-central" 2381 2382
start_member "ap-south" 2383 2384

echo "[start_etcd_cluster] Waiting for 3-member cluster readiness..."
READY=0
for i in {1..30}; do
    if ETCDCTL_API=3 "${ETCDCTL_BIN}" \
        --cacert="${CERTS_DIR}/ca.crt" \
        --cert="${CERTS_DIR}/client.crt" \
        --key="${CERTS_DIR}/client.key" \
        --endpoints="https://127.0.0.1:2379,https://127.0.0.1:2381,https://127.0.0.1:2383" \
        endpoint health 2>/dev/null | grep -c "is healthy" | grep -q "3"; then
        READY=1
        break
    fi
    sleep 0.5
done

if [[ "${READY}" -ne 1 ]]; then
    echo "ERROR: etcd cluster failed to become healthy within 15 seconds!"
    echo "Logs:"
    tail -n 20 "${DATA_DIR}"/*.log || true
    "${ROOT_DIR}/scripts/stop_etcd_cluster.sh"
    exit 1
fi

echo "[start_etcd_cluster] Cluster is HEALTHY. All 3 members active with mTLS."
ETCDCTL_API=3 "${ETCDCTL_BIN}" \
    --cacert="${CERTS_DIR}/ca.crt" \
    --cert="${CERTS_DIR}/client.crt" \
    --key="${CERTS_DIR}/client.key" \
    --endpoints="https://127.0.0.1:2379,https://127.0.0.1:2381,https://127.0.0.1:2383" \
    endpoint status --write-out=table
