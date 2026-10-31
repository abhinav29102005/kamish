"""Real Rust CLI smoke tests against the synthetic SDK broker fixture."""
from pathlib import Path
import argparse
import json
import os
import selectors
import signal
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
parser = argparse.ArgumentParser()
parser.add_argument("--output-dir", type=Path, default=HERE)
RESULT_ROOT = parser.parse_args().output_dir.resolve()
OUTPUT = RESULT_ROOT / "results"
OUTPUT.mkdir(exist_ok=True, parents=True)
subprocess.run(["cargo", "build", "-p", "kamish-client", "--example", "sdk_test_broker"],
               cwd=ROOT, check=True)
subprocess.run(["cargo", "build", "-p", "kamish-ctl"], cwd=ROOT, check=True)
summary = []
with tempfile.TemporaryDirectory(prefix="kamish-cli-test-", dir="/tmp/opencode") as directory:
    broker = subprocess.Popen([str(ROOT / "target/debug/examples/sdk_test_broker"), directory],
                              cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        selector = selectors.DefaultSelector()
        selector.register(broker.stdout, selectors.EVENT_READ)
        if not selector.select(timeout=10):
            raise RuntimeError("Synthetic broker did not report readiness")
        endpoint = json.loads(broker.stdout.readline())
        selector.close()
        addr = f"{endpoint['host']}:{endpoint['port']}"
        common = ["--addr", addr, "--topic", "cli-events", "--partition", "0"]
        tests = [
            ("cli-produce-first", ["produce", *common, "--class-id", "general", "--payload", "first-cli-event", "--region", "EU_WEST", "--policy", "strict-local"], 0),
            ("cli-consume", ["consume", *common, "--class-id", "general", "--offset", "0", "--max-records", "10"], 0),
            ("cli-watermark", ["watermark", *common, "--source-region", "EU_WEST"], 0),
            # A normal second CLI produce should use a distinct/new producer
            # session or resume its sequence, not reuse producer 1001/epoch 1/seq 0.
            ("cli-produce-second", ["produce", *common, "--class-id", "general", "--payload", "second-cli-event", "--region", "EU_WEST", "--policy", "strict-local"], 0),
            # Verification must not quietly create the nonexistent evidence it
            # was asked to verify and call that a valid audit.
            ("cli-verify-missing-evidence", ["verify-audit", "--log-path", str(Path(directory) / "does-not-exist.log")], 1),
        ]
        for name, arguments, expected in tests:
            command = [str(ROOT / "target/debug/kamish-ctl"), *arguments]
            result = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE,
                                    stderr=subprocess.STDOUT, text=True, timeout=30)
            (OUTPUT / f"{name}.txt").write_text(result.stdout)
            passed = (result.returncode == 0) if expected == 0 else (result.returncode != 0)
            summary.append({"name": name, "exit_code": result.returncode,
                            "expected_success": expected == 0, "contract_passed": passed,
                            "arguments": arguments, "output": f"results/{name}.txt"})
            print(f"{name}: {'PASS' if passed else 'FAIL'} (exit {result.returncode})", flush=True)
    finally:
        if broker.poll() is None:
            broker.send_signal(signal.SIGINT)
            try:
                broker.wait(timeout=5)
            except subprocess.TimeoutExpired:
                broker.kill()
                broker.wait()
(RESULT_ROOT / "cli-smoke.json").write_text(json.dumps(summary, indent=2) + "\n")
sys.exit(1 if any(not check["contract_passed"] for check in summary) else 0)
