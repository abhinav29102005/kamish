#!/usr/bin/env bash
set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PID_DIR="${ROOT_DIR}/data/etcd/pids"

if [[ -d "${PID_DIR}" ]]; then
    for pidfile in "${PID_DIR}"/*.pid; do
        if [[ -f "${pidfile}" ]]; then
            PID=$(cat "${pidfile}")
            if kill -0 "${PID}" 2>/dev/null; then
                echo "[stop_etcd_cluster] Stopping PID ${PID} ($(basename "${pidfile}" .pid))..."
                kill -15 "${PID}" 2>/dev/null || true
                for i in {1..30}; do
                    if ! kill -0 "${PID}" 2>/dev/null; then
                        break
                    fi
                    sleep 0.1
                done
                if kill -0 "${PID}" 2>/dev/null; then
                    echo "[stop_etcd_cluster] Force killing PID ${PID}..."
                    kill -9 "${PID}" 2>/dev/null || true
                fi
            fi
            rm -f "${pidfile}"
        fi
    done
fi

pkill -f "initial-cluster-token kamish-authority-test" 2>/dev/null || true
sleep 0.5

if [[ "${1:-}" == "--clean-data" ]]; then
    echo "[stop_etcd_cluster] Cleaning etcd data directory..."
    rm -rf "${ROOT_DIR}/data/etcd"
fi

echo "[stop_etcd_cluster] Cleaned up etcd processes."
