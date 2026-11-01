#!/usr/bin/env python3
import os
import sys
import shutil
import subprocess
import time
import socket
import signal

LOG_ENTRIES = []

def log(section, cmd, output, code):
    entry = f"=== [{section}] ===\n$ {cmd}\nExit Code: {code}\nOutput:\n{output}\n"
    print(entry)
    LOG_ENTRIES.append(entry)

def run_cmd(section, cmd, cwd=None, env=None, check=True):
    res = subprocess.run(cmd, shell=True, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    log(section, cmd, res.stdout, res.returncode)
    if check and res.returncode != 0:
        raise RuntimeError(f"Command failed with code {res.returncode}: {cmd}")
    return res

def wait_for_port(port, timeout=10.0):
    start = time.time()
    while time.time() - start < timeout:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.5):
                return True
        except OSError:
            time.sleep(0.2)
    return False

def main():
    subprocess.run("pkill -9 -f 'kamish start' || true", shell=True)
    time.sleep(0.5)

    clean_dir = "/tmp/kamish_cleanroom"
    global_prefix = os.path.join(clean_dir, "global_prefix")
    sdk_test = os.path.join(clean_dir, "sdk_test")
    cluster_dir = os.path.join(clean_dir, "cluster")

    shutil.rmtree(global_prefix, ignore_errors=True)
    shutil.rmtree(sdk_test, ignore_errors=True)
    shutil.rmtree(cluster_dir, ignore_errors=True)

    os.makedirs(global_prefix, exist_ok=True)
    os.makedirs(sdk_test, exist_ok=True)
    os.makedirs(cluster_dir, exist_ok=True)

    # 0. Ensure native binary and packaged tarballs exist
    if not os.path.exists("packages/kamish-linux-x64/bin/kamish") or os.path.getsize("packages/kamish-linux-x64/bin/kamish") < 1024:
        run_cmd("Build Release Binary", "cargo build --release -p kamish && cp target/release/kamish packages/kamish-linux-x64/bin/kamish")
    for pkg in ["packages/kamish-client", "packages/kamish-linux-x64", "packages/kamish"]:
        run_cmd(f"Pack {pkg}", f"npm pack --pack-destination {clean_dir}", cwd=pkg)

    # 1. SDK Import & Typecheck Test
    print("--- 1. Testing SDK Package Installation and Typechecking ---")
    run_cmd("SDK Test Init", "npm init -y", cwd=sdk_test)
    run_cmd("SDK Install", f"npm install {clean_dir}/kamish-client-0.1.0-alpha.1.tgz", cwd=sdk_test)
    
    test_ts = """import { KamishClient } from "kamish-client";
const client = new KamishClient({ allowInsecure: true, host: "127.0.0.1", port: 9092 });
console.log("SDK client instantiated successfully with maxPayloadBytes:", client.maxPayloadBytes);
"""
    with open(os.path.join(sdk_test, "test.ts"), "w") as f:
        f.write(test_ts)
    
    run_cmd("SDK Typecheck", "npx -y typescript --noEmit test.ts", cwd=sdk_test)

    # 2. Global CLI Install from Tarball
    print("--- 2. Testing Global CLI Tarball Installation ---")
    run_cmd(
        "NPM Global Install",
        "npm install -g --prefix " + global_prefix + " " + (f"{clean_dir}/kamish-linux-x64-0.1.0-alpha.1.tgz" if os.path.exists(f"{clean_dir}/kamish-linux-x64-0.1.0-alpha.1.tgz") else f"{clean_dir}/kamish-kamish-linux-x64-0.1.0-alpha.1.tgz") + f" {clean_dir}/kamish-client-0.1.0-alpha.1.tgz {clean_dir}/kamish-0.1.0-alpha.1.tgz",
        cwd=clean_dir
    )

    env = dict(os.environ)
    env["PATH"] = f"{global_prefix}/bin:{env['PATH']}"

    # 3. CLI --help and --version
    print("--- 3. Testing CLI Help & Version ---")
    run_cmd("CLI Help", "kamish --help", cwd=cluster_dir, env=env)
    run_cmd("CLI Version", "kamish --version", cwd=cluster_dir, env=env)

    # 4. kamish doctor
    print("--- 4. Testing System Prerequisites (kamish doctor) ---")
    run_cmd("CLI Doctor", "kamish doctor", cwd=cluster_dir, env=env, check=False)

    # 5. kamish init
    print("--- 5. Testing Cluster Initialization (kamish init) ---")
    run_cmd(
        "CLI Init",
        "kamish init --admin-user clean_admin --admin-password CleanroomPw2026! --region EU_CENTRAL",
        cwd=cluster_dir,
        env=env
    )

    # 6. Verify Policy Sign & Verify
    print("--- 6. Testing Policy Verification (kamish policy verify) ---")
    run_cmd(
        "CLI Policy Verify",
        "kamish policy verify --policy policy.signed.json --trusted-keys admin.pub",
        cwd=cluster_dir,
        env=env
    )

    # 7. Start Daemon & Test Broker + Gateway + Dashboard
    print("--- 7. Launching kamish start in background ---")
    start_cmd = ["kamish", "start", "--port", "3199", "--broker-port", "9192"]
    daemon_proc = subprocess.Popen(
        start_cmd,
        cwd=cluster_dir,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        start_new_session=True
    )

    try:
        if not wait_for_port(3199, timeout=8.0):
            out, _ = daemon_proc.communicate(timeout=2)
            raise RuntimeError(f"Gateway port 3199 did not bind in time. Daemon output:\n{out}")
        if not wait_for_port(9192, timeout=8.0):
            out, _ = daemon_proc.communicate(timeout=2)
            raise RuntimeError(f"Broker port 9192 did not bind in time. Daemon output:\n{out}")

        print("Ports 3199 and 9192 active! Running operational commands...")

        # kamish status
        run_cmd(
            "CLI Status",
            "kamish status --gateway-url http://127.0.0.1:3199 --username clean_admin --password CleanroomPw2026!",
            cwd=cluster_dir,
            env=env
        )

        # kamish topic list
        run_cmd(
            "CLI Topic List",
            "kamish topic list --gateway-url http://127.0.0.1:3199 --username clean_admin --password CleanroomPw2026!",
            cwd=cluster_dir,
            env=env
        )

        # kamish produce
        run_cmd(
            "CLI Produce",
            "kamish produce --addr 127.0.0.1:9192 --topic general-topic --partition 0 --class-id general --payload '{\"tx_id\":\"clean-tx-001\",\"amount\":42}' --region EU_CENTRAL",
            cwd=cluster_dir,
            env=env
        )

        # kamish fetch
        run_cmd(
            "CLI Fetch",
            "kamish fetch --addr 127.0.0.1:9192 --topic general-topic --partition 0 --class-id general --offset 0 --max-records 1",
            cwd=cluster_dir,
            env=env
        )

        # kamish verify-audit
        run_cmd(
            "CLI Verify Audit",
            "kamish verify-audit --log-path ./data/kamish/audit.log",
            cwd=cluster_dir,
            env=env
        )

    finally:
        print("Sending SIGTERM to kamish daemon process group...")
        try:
            os.killpg(os.getpgid(daemon_proc.pid), signal.SIGTERM)
            out, _ = daemon_proc.communicate(timeout=5)
            log("Daemon Shutdown", "kamish start", out, daemon_proc.returncode)
        except Exception as e:
            try:
                os.killpg(os.getpgid(daemon_proc.pid), signal.SIGKILL)
            except Exception:
                pass
            print(f"Exception during daemon cleanup: {e}")

    # 8. Test Data Persistence & Schema Format Check (Upgrade Simulation)
    print("--- 8. Testing Upgrade Path & Format Compatibility ---")
    data_log = os.path.join(cluster_dir, "data", "kamish", "audit.log")
    if not os.path.exists(data_log):
        raise RuntimeError(f"Expected persisted audit log at {data_log}")
    run_cmd(
        "CLI Audit Post-Shutdown Persistence",
        f"kamish verify-audit --log-path {data_log}",
        cwd=cluster_dir,
        env=env
    )

    # 9. Uninstall Test
    print("--- 9. Testing Global Uninstall ---")
    run_cmd(
        "NPM Global Uninstall",
        f"npm uninstall -g --prefix {global_prefix} kamish",
        cwd=clean_dir
    )

    kamish_bin = os.path.join(global_prefix, "bin", "kamish")
    if os.path.exists(kamish_bin):
        raise RuntimeError(f"kamish binary was not removed: {kamish_bin}")
    print(f"Verified {kamish_bin} successfully uninstalled!")

    # Write transcript to audit file
    os.makedirs("audit", exist_ok=True)
    transcript_path = "audit/clean_room_install_transcript.txt"
    with open(transcript_path, "w") as f:
        f.write("\n".join(LOG_ENTRIES))
    print(f"\nAll Clean-Room Installation & Verification tests passed! Transcript saved to {transcript_path}")

if __name__ == "__main__":
    main()
