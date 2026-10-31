#!/usr/bin/env bash
set -euo pipefail

ETCD_VERSION="v3.7.3"
ETCD_SHA256="5dfa9cc3b3ce3b4b491fbcc2057d3daa166bcc6676f743fcd20f8e5760cb9775"
ARCH="linux-amd64"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${ROOT_DIR}/bin"
ETCD_BIN="${BIN_DIR}/etcd"

if [[ -x "${ETCD_BIN}" ]]; then
    CURRENT_VER=$("${ETCD_BIN}" --version | grep "etcd Version:" | awk '{print $3}')
    if [[ "${CURRENT_VER}" == "3.7.3" ]]; then
        echo "[download_etcd] etcd 3.7.3 already present at ${ETCD_BIN}"
        exit 0
    fi
fi

mkdir -p "${BIN_DIR}"
TARBALL="${BIN_DIR}/etcd-${ETCD_VERSION}-${ARCH}.tar.gz"
DOWNLOAD_URL="https://github.com/etcd-io/etcd/releases/download/${ETCD_VERSION}/etcd-${ETCD_VERSION}-${ARCH}.tar.gz"

echo "[download_etcd] Downloading etcd ${ETCD_VERSION} from ${DOWNLOAD_URL}..."
curl -4 -sL -o "${TARBALL}" "${DOWNLOAD_URL}"

echo "[download_etcd] Verifying SHA256 checksum..."
ACTUAL_SHA256=$(sha256sum "${TARBALL}" | awk '{print $1}')
if [[ "${ACTUAL_SHA256}" != "${ETCD_SHA256}" ]]; then
    echo "ERROR: SHA256 mismatch! Expected ${ETCD_SHA256}, got ${ACTUAL_SHA256}"
    rm -f "${TARBALL}"
    exit 1
fi
echo "[download_etcd] Checksum verified: ${ACTUAL_SHA256}"

tar -xzf "${TARBALL}" -C "${BIN_DIR}" --strip-components=1 "etcd-${ETCD_VERSION}-${ARCH}/etcd" "etcd-${ETCD_VERSION}-${ARCH}/etcdctl"
rm -f "${TARBALL}"
chmod +x "${BIN_DIR}/etcd" "${BIN_DIR}/etcdctl"

echo "[download_etcd] etcd ${ETCD_VERSION} installed successfully at ${BIN_DIR}/etcd"
"${BIN_DIR}/etcd" --version
