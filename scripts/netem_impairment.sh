#!/usr/bin/env bash
# Netem Network Impairment & Calibration Script (W31)
# Configures kernel-level network impairment via tc netem or network namespaces.
set -euo pipefail

CMD="${1:-help}"
IFACE="${2:-lo}"
DELAY_MS="${3:-100}"
JITTER_MS="${4:-5}"
LOSS_PCT="${5:-0}"

case "$CMD" in
    apply)
        echo ">> Applying tc netem on ${IFACE}: delay ${DELAY_MS}ms ± ${JITTER_MS}ms, loss ${LOSS_PCT}%..."
        if command -v tc >/dev/null 2>&1; then
            if sudo -n true 2>/dev/null; then
                sudo tc qdisc del dev "${IFACE}" root 2>/dev/null || true
                sudo tc qdisc add dev "${IFACE}" root netem delay "${DELAY_MS}ms" "${JITTER_MS}ms" loss "${LOSS_PCT}%"
                echo ">> Applied successfully via tc netem."
            else
                echo ">> Note: sudo requires password; use userspace ImpairedTcpProxy for non-root test environments."
            fi
        else
            echo ">> tc tool not found in PATH; falling back to userspace ImpairedTcpProxy."
        fi
        ;;

    clear)
        echo ">> Clearing tc netem on ${IFACE}..."
        if command -v tc >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
            sudo tc qdisc del dev "${IFACE}" root 2>/dev/null || true
            echo ">> Cleared."
        fi
        ;;

    status)
        if command -v tc >/dev/null 2>&1; then
            tc -s qdisc show dev "${IFACE}" || true
        fi
        ;;

    calibrate)
        echo ">> Running automated transport calibration via kamish-ctl..."
        cargo run --bin kamish-ctl -- benchmark --count 100 --concurrency 1
        ;;

    *)
        echo "Usage: $0 {apply|clear|status|calibrate} [interface] [delay_ms] [jitter_ms] [loss_pct]"
        exit 1
        ;;
esac
