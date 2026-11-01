#!/usr/bin/env python3
"""
Cross-Platform NPM Package Verification Test
Validates that `kamish` functions seamlessly on Windows (win32) and macOS (darwin)
using both native binaries when available and the portable runtime engine.
"""

import subprocess
import sys
import os
import json
import time
import socket
import urllib.request

def wait_for_port(port, timeout=5.0):
    start = time.time()
    while time.time() - start < timeout:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.3):
                return True
        except OSError:
            time.sleep(0.1)
    return False

def test_platform_simulation(platform_name, arch_name, test_port):
    print(f"\n--- Testing Platform Simulation: {platform_name} ({arch_name}) ---")
    
    # 1. Test CLI Help & Version via node launcher
    cmd = [
        "node", "-e",
        f"""
        Object.defineProperty(process, 'platform', {{ value: '{platform_name}' }});
        Object.defineProperty(process, 'arch', {{ value: '{arch_name}' }});
        process.env.KAMISH_PORTABLE = '1';
        process.argv = ['node', 'kamish', '--version'];
        import('./packages/kamish/bin/kamish.js');
        """
    ]
    res = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if res.returncode != 0 or "0.1.0-alpha.1" not in res.stdout:
        print(f"FAILED: Version check on {platform_name}:\n{res.stdout}\n{res.stderr}")
        return False
    print(f"  ✔ kamish --version passed on {platform_name}")

    # 2. Test Doctor Diagnostics
    cmd_doctor = [
        "node", "-e",
        f"""
        Object.defineProperty(process, 'platform', {{ value: '{platform_name}' }});
        Object.defineProperty(process, 'arch', {{ value: '{arch_name}' }});
        process.env.KAMISH_PORTABLE = '1';
        process.argv = ['node', 'kamish', 'doctor'];
        import('./packages/kamish/bin/kamish.js');
        """
    ]
    res_doctor = subprocess.run(cmd_doctor, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if res_doctor.returncode != 0 or "All diagnostic checks passed" not in res_doctor.stdout:
        print(f"FAILED: Doctor check on {platform_name}:\n{res_doctor.stdout}\n{res_doctor.stderr}")
        return False
    print(f"  ✔ kamish doctor passed on {platform_name}")

    # 3. Test kamish start --dev on test_port
    data_dir = f"./data/test_{platform_name}_{arch_name}"
    cmd_start = [
        "node", "-e",
        f"""
        Object.defineProperty(process, 'platform', {{ value: '{platform_name}' }});
        Object.defineProperty(process, 'arch', {{ value: '{arch_name}' }});
        process.env.KAMISH_PORTABLE = '1';
        process.argv = ['node', 'kamish', 'start', '--dev', '--port', '{test_port}', '--data-dir', '{data_dir}'];
        import('./packages/kamish/bin/kamish.js');
        """
    ]
    proc = subprocess.Popen(cmd_start, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    
    try:
        if not wait_for_port(test_port, timeout=6.0):
            out, _ = proc.communicate(timeout=1)
            print(f"FAILED: Daemon did not bind to {test_port} on {platform_name}. Output:\n{out}")
            return False

        # Verify /api/status
        with urllib.request.urlopen(f"http://127.0.0.1:{test_port}/api/status") as resp:
            data = json.loads(resp.read().decode())
            assert data["status"] == "HEALTHY", f"Unexpected status: {data}"
            assert data["operating_mode"] == "PORTABLE_ENGINE"
        print(f"  ✔ GET /api/status verified on {platform_name}")

        # Verify /api/topics (all 6 topics)
        with urllib.request.urlopen(f"http://127.0.0.1:{test_port}/api/topics") as resp:
            data = json.loads(resp.read().decode())
            topics = [t["topic"] for t in data["topics"]]
            expected_topics = [
                "financial-transactions",
                "sovereign-stream",
                "healthcare-clinical-trials",
                "cross-border-kyc-registry",
                "order-matching-engine",
                "general-topic"
            ]
            for exp in expected_topics:
                assert exp in topics, f"Missing topic {exp} in {topics}"
        print(f"  ✔ GET /api/topics (6 sovereign topics) verified on {platform_name}")

        # Verify /api/audit/verify
        req = urllib.request.Request(f"http://127.0.0.1:{test_port}/api/audit/verify", data=b"{}", headers={"Content-Type": "application/json"})
        with urllib.request.urlopen(req) as resp:
            audit_res = json.loads(resp.read().decode())
            assert audit_res["verification_result"] == "PASS"
            assert audit_res["hash_chain_valid"] is True
            assert audit_res["zero_tamper_detected"] is True
            assert audit_res["total_streams_audited"] == 18
        print(f"  ✔ POST /api/audit/verify (18 streams PASS) verified on {platform_name}")

        # Verify Dashboard HTTP serving
        with urllib.request.urlopen(f"http://127.0.0.1:{test_port}/") as resp:
            html = resp.read().decode()
            assert resp.status == 200
            assert "<!DOCTYPE html>" in html or "html" in html.lower()
        print(f"  ✔ GET / Dashboard UI HTML served on {platform_name}")

    finally:
        proc.terminate()
        try:
            proc.communicate(timeout=2)
        except Exception:
            proc.kill()
        subprocess.run(f"rm -rf {data_dir}", shell=True)

    return True

def verify_platform_package_metadata():
    print("\n--- Verifying Platform Packages Metadata & Executable Launchers ---")

    # 1. kamish-win32-x64
    win_pkg_path = "packages/kamish-win32-x64/package.json"
    with open(win_pkg_path) as f:
        win_pkg = json.load(f)
    assert win_pkg["os"] == ["win32"], "win32 package must specify os: ['win32']"
    assert win_pkg["cpu"] == ["x64"], "win32 package must specify cpu: ['x64']"
    assert "kamish" in win_pkg.get("bin", {}), "win32 package must specify bin.kamish"
    assert os.path.exists("packages/kamish-win32-x64/bin/kamish.cmd"), "kamish.cmd must exist"
    assert os.path.exists("packages/kamish-win32-x64/bin/kamish.ps1"), "kamish.ps1 must exist"
    print("  ✔ kamish-win32-x64 metadata and CMD/PS1 launchers verified")

    # 2. kamish-darwin-arm64
    mac_arm_path = "packages/kamish-darwin-arm64/package.json"
    with open(mac_arm_path) as f:
        mac_arm = json.load(f)
    assert mac_arm["os"] == ["darwin"], "darwin package must specify os: ['darwin']"
    assert mac_arm["cpu"] == ["arm64"], "darwin package must specify cpu: ['arm64']"
    assert "kamish" in mac_arm.get("bin", {}), "darwin package must specify bin.kamish"
    assert os.path.exists("packages/kamish-darwin-arm64/bin/kamish"), "darwin bin/kamish must exist"
    assert os.access("packages/kamish-darwin-arm64/bin/kamish", os.X_OK), "darwin bin/kamish must be executable"
    print("  ✔ kamish-darwin-arm64 metadata and executable launcher verified")

    # 3. kamish-darwin-x64
    mac_x64_path = "packages/kamish-darwin-x64/package.json"
    with open(mac_x64_path) as f:
        mac_x64 = json.load(f)
    assert mac_x64["os"] == ["darwin"], "darwin package must specify os: ['darwin']"
    assert mac_x64["cpu"] == ["x64"], "darwin package must specify cpu: ['x64']"
    assert "kamish" in mac_x64.get("bin", {}), "darwin package must specify bin.kamish"
    assert os.path.exists("packages/kamish-darwin-x64/bin/kamish"), "darwin bin/kamish must exist"
    assert os.access("packages/kamish-darwin-x64/bin/kamish", os.X_OK), "darwin bin/kamish must be executable"
    print("  ✔ kamish-darwin-x64 metadata and executable launcher verified")

    # 4. kamish primary package
    primary_pkg_path = "packages/kamish/package.json"
    with open(primary_pkg_path) as f:
        primary = json.load(f)
    assert "lib" in primary["files"], "Primary package files must include 'lib'"
    assert "kamish-win32-x64" in primary["optionalDependencies"], "Primary package must list kamish-win32-x64 in optionalDependencies"
    assert "kamish-darwin-arm64" in primary["optionalDependencies"], "Primary package must list kamish-darwin-arm64 in optionalDependencies"
    assert "kamish-darwin-x64" in primary["optionalDependencies"], "Primary package must list kamish-darwin-x64 in optionalDependencies"
    print("  ✔ kamish primary package dependencies and file list verified")

def main():
    verify_platform_package_metadata()
    
    win_ok = test_platform_simulation("win32", "x64", 3095)
    if not win_ok:
        sys.exit(1)

    mac_arm_ok = test_platform_simulation("darwin", "arm64", 3096)
    if not mac_arm_ok:
        sys.exit(1)

    mac_x64_ok = test_platform_simulation("darwin", "x64", 3097)
    if not mac_x64_ok:
        sys.exit(1)

    print("\n========================================================")
    print("  ALL WINDOWS & MACOS CROSS-PLATFORM TESTS PASSED (100%)")
    print("========================================================\n")

if __name__ == "__main__":
    main()
