"""Reproducible local checks. Records failures without modifying application code.

Run from any directory: python3 audit/application_testing_2026-11-01/run_checks.py
No publish, deployment, credential configuration or git mutation is performed.
"""
from datetime import datetime, timezone
import argparse
from pathlib import Path
import json
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
SDK = ROOT / "packages/kamish-client"
FRONTEND = ROOT / "frontend"
parser = argparse.ArgumentParser()
parser.add_argument("--output-dir", type=Path, default=HERE)
args = parser.parse_args()
RESULT_ROOT = args.output_dir.resolve()
OUTPUT = RESULT_ROOT / "results"
OUTPUT.mkdir(exist_ok=True, parents=True)

CHECKS = [
    ("rust-check", ROOT, ["cargo", "check", "--workspace", "--all-targets"]),
    ("rust-tests", ROOT, ["cargo", "test", "--workspace"]),
    ("research-probes", ROOT, ["cargo", "test", "--locked", "--manifest-path", "early_research/probes/Cargo.toml", "--target-dir", "target", "--", "--test-threads=1"]),
    ("rust-format", ROOT, ["cargo", "fmt", "--all", "--", "--check"]),
    ("rust-clippy", ROOT, ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]),
    ("broker-help", ROOT, ["cargo", "run", "-p", "kamish-broker", "--", "--help"]),
    ("agent-help", ROOT, ["cargo", "run", "-p", "kamish-agent", "--", "--help"]),
    ("ctl-help", ROOT, ["cargo", "run", "-p", "kamish-ctl", "--", "--help"]),
    *[(f"ctl-{name}-help", ROOT, ["cargo", "run", "-p", "kamish-ctl", "--", name, "--help"])
      for name in ["produce", "consume", "watermark", "verify-audit", "demo", "benchmark"]],
    ("demo-script", ROOT, ["bash", "scripts/run_demo.sh"]),
    ("benchmark-smoke", ROOT, ["cargo", "run", "-p", "kamish-ctl", "--", "benchmark", "--count", "64", "--concurrency", "2"]),
    ("sdk-typecheck", SDK, ["npm", "run", "typecheck"]),
    ("sdk-tests", SDK, ["npm", "test"]),
    ("sdk-rust-integration", SDK, ["npm", "run", "test:integration"]),
    ("sdk-package-consumer", SDK, ["npm", "run", "test:package"]),
    ("sdk-runtime-audit", SDK, ["npm", "audit", "--omit=dev", "--json"]),
    ("frontend-build", FRONTEND, ["npm", "run", "build"]),
    ("frontend-lint", FRONTEND, ["npm", "run", "lint"]),
    ("frontend-tests", FRONTEND, ["npm", "test"]),
    # Use the frontend's own locked compiler; do not substitute the SDK compiler.
    ("frontend-typecheck", FRONTEND, ["npm", "run", "typecheck"]),
    ("git-whitespace", ROOT, ["git", "diff", "--check"]),
]

summary = {
    "started_at": datetime.now(timezone.utc).isoformat(),
    "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
    "checks": [],
}
for name, cwd, command in CHECKS:
    started = time.monotonic()
    try:
        result = subprocess.run(command, cwd=cwd, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=180, text=True)
        code, output = result.returncode, result.stdout
    except subprocess.TimeoutExpired as error:
        code = 124
        output = error.stdout or b""
        if isinstance(output, bytes):
            output = output.decode("utf-8", errors="replace")
        output += "\nCHECK TIMED OUT\n"
    except OSError as error:
        code, output = 127, str(error)
    (OUTPUT / f"{name}.txt").write_text(output)
    summary["checks"].append({
        "name": name, "command": command, "cwd": str(cwd.relative_to(ROOT)) or ".",
        "exit_code": code, "duration_seconds": round(time.monotonic() - started, 3),
        "output": f"results/{name}.txt",
    })
    print(f"{name}: exit {code}", flush=True)
summary["finished_at"] = datetime.now(timezone.utc).isoformat()
(RESULT_ROOT / "checks.json").write_text(json.dumps(summary, indent=2) + "\n")
print("Local checks recorded; read REPORT.md for interpretation, warnings and coverage limits.")
sys.exit(1 if any(check["exit_code"] != 0 for check in summary["checks"]) else 0)
