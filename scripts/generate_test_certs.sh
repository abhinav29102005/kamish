#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CERTS_DIR="${ROOT_DIR}/certs"

mkdir -p "${CERTS_DIR}"
cd "${CERTS_DIR}"

echo "[generate_test_certs] Generating Test Root CA..."
openssl req -new -x509 -days 30 -nodes \
    -subj "/CN=KamishTestCA/O=Kamish" \
    -keyout ca.key -out ca.crt 2>/dev/null

# OpenSSL config for SANs
cat << 'CONFIG' > openssl.cnf
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
CN = localhost
O = Kamish

[v3_req]
basicConstraints = CA:FALSE
keyUsage = nonRepudiation, digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth, clientAuth
subjectAltName = @alt_names

[alt_names]
DNS.1 = localhost
DNS.2 = etcd-us-east
DNS.3 = etcd-eu-central
DNS.4 = etcd-ap-south
IP.1 = 127.0.0.1
CONFIG

# Generate per-member certs
MEMBERS=("us-east" "eu-central" "ap-south")
for MEMBER in "${MEMBERS[@]}"; do
    openssl req -new -nodes -config openssl.cnf \
        -keyout "${MEMBER}.key" -out "${MEMBER}.csr" 2>/dev/null
    openssl x509 -req -days 30 -in "${MEMBER}.csr" -CA ca.crt -CAkey ca.key -CAcreateserial \
        -extfile openssl.cnf -extensions v3_req \
        -out "${MEMBER}.crt" 2>/dev/null
    rm -f "${MEMBER}.csr"
done

# Generate client cert
openssl req -new -nodes -config openssl.cnf \
    -keyout client.key -out client.csr 2>/dev/null
openssl x509 -req -days 30 -in client.csr -CA ca.crt -CAkey ca.key -CAcreateserial \
    -extfile openssl.cnf -extensions v3_req \
    -out client.crt 2>/dev/null
rm -f client.csr openssl.cnf ca.srl

chmod 600 *.key
echo "[generate_test_certs] Successfully generated test CA, member certs, and client cert in ${CERTS_DIR}"
