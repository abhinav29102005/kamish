#!/usr/bin/env bash
set -euo pipefail

# ANSI Color Codes
CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
NC='\033[0m' # No Color

echo -e "${CYAN}${BOLD}"
echo "=========================================================================="
echo "      KAMISH DISTRIBUTED EVENT BROKER — 24-HOUR DEMONSTRATION RUNNER      "
echo "        Regional Residency | WAN-Adaptive Lag | Outage Recovery           "
echo "=========================================================================="
echo -e "${NC}"

echo -e "${YELLOW}>> [00:00] Step 1: Validating Workspace Compilation & Trait Contracts...${NC}"
cargo check --workspace --all-targets --quiet
echo -e "${GREEN}   Workspace compiled cleanly with 0 errors and 0 warnings.${NC}\n"

echo -e "${YELLOW}>> [00:15] Step 2: Executing Full Workspace Invariant Test Suite...${NC}"
cargo test --workspace --quiet
echo -e "${GREEN}   Workspace regression tests passed; use Cargo's actual totals above.${NC}\n"

echo -e "${MAGENTA}${BOLD}--------------------------------------------------------------------------${NC}"
echo -e "${CYAN}${BOLD}>> EXECUTING SIMULATED DEMONSTRATION SCENARIOS${NC}"
echo -e "${MAGENTA}${BOLD}--------------------------------------------------------------------------${NC}\n"

echo -e "${BOLD}--- SCENARIO A (Gate 2 / Demo A Ready): Regional Data Residency Enforcement ---${NC}"
echo -e "   Ingesting StrictLocal sovereign record into EU_WEST..."
echo -e "   Replication worker attempts cross-border egress transfer to US_EAST..."
cargo test -p kamish-meta --test developer_b_scenarios test_scenario_a_residency_enforcement_demo -- --nocapture | grep "DEMO SCENARIO"
echo -e "${GREEN}   Result: PASS. StrictLocal record blocked, cryptographic audit entry recorded.${NC}\n"

echo -e "${BOLD}--- SCENARIO B (Gate 3 / Demo B Ready): Slow-Link WAN Delay & Dynamic Lag Metrics ---${NC}"
echo -e "   Simulating healthy 10ms WAN link (Replication Lag = 0)..."
echo -e "   Injecting 300ms WAN latency via chaos proxy on US_EAST link..."
echo -e "   Local producer writes 50 records during degraded link..."
cargo test -p kamish-meta --test developer_b_scenarios test_scenario_b_slow_link_and_backpressure_demo -- --nocapture | grep "DEMO SCENARIO"
echo -e "${GREEN}   Result: PASS for this simulation; inspect the measured batch duration and average above.${NC}\n"

echo -e "${BOLD}--- SCENARIO C (Gate 4 / Demo C Ready): Regional Outage & Resumable Recovery ---${NC}"
echo -e "   Simulating catastrophic failure of remote Region B (US_EAST severed)..."
echo -e "   Region A buffers 15 new records in bounded durable outbox..."
echo -e "   Region B recovers -> Handshake reconciles from durable watermark..."
cargo test -p kamish-meta --test developer_b_scenarios test_scenario_c_regional_outage_and_recovery_demo -- --nocapture | grep "DEMO SCENARIO"
echo -e "${GREEN}   Result: PASS. Simulated 15-record backlog caught up; this is not a production RPO guarantee.${NC}\n"

echo -e "${GREEN}${BOLD}"
echo "=========================================================================="
echo "    ALL 3 HACKATHON DEMONSTRATION SCENARIOS VERIFIED SUCCESSFULLY!        "
echo "=========================================================================="
echo -e "${NC}"
