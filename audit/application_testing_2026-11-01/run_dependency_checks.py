"""Record dependency checks without installing/upgrading packages."""
from pathlib import Path
import argparse
import json
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
parser = argparse.ArgumentParser()
parser.add_argument("--output-dir", type=Path, default=HERE)
RESULT_ROOT = parser.parse_args().output_dir.resolve()
(RESULT_ROOT / "results").mkdir(exist_ok=True, parents=True)
summary = []
for name, command in [
    ("frontend-dependency-tree", ["npm", "ls", "--depth=0", "--json"]),
    ("frontend-dependency-audit", ["npm", "audit", "--json"]),
]:
    result = subprocess.run(command, cwd=ROOT / "frontend", stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, timeout=120)
    (RESULT_ROOT / "results" / f"{name}.txt").write_text(result.stdout)
    summary.append({"name": name, "exit_code": result.returncode,
                    "command": command, "output": f"results/{name}.txt"})
    print(f"{name}: exit {result.returncode}", flush=True)
(RESULT_ROOT / "dependency-checks.json").write_text(json.dumps(summary, indent=2) + "\n")
sys.exit(1 if any(check["exit_code"] != 0 for check in summary) else 0)
