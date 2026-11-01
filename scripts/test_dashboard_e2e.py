#!/usr/bin/env python3
"""
Kamish Dashboard End-to-End Browser Test (Phase 5).
Uses Playwright to test the live React dashboard connected to the real kamish-gateway daemon.
"""

import os
import sys
import time
import shutil
import tempfile
import subprocess
import urllib.request
import urllib.error
from playwright.sync_api import sync_playwright

GATEWAY_PORT = 3001
FRONTEND_PORT = 5173
GATEWAY_URL = f"http://127.0.0.1:{GATEWAY_PORT}"
FRONTEND_URL = f"http://127.0.0.1:{FRONTEND_PORT}"

def wait_for_service(url, timeout=15):
    start = time.time()
    while time.time() - start < timeout:
        try:
            with urllib.request.urlopen(url, timeout=1) as resp:
                if resp.status in (200, 401, 403, 404):
                    return True
        except urllib.error.HTTPError as e:
            if e.code in (200, 401, 403, 404):
                return True
        except Exception:
            pass
        time.sleep(0.3)
    return False

def run_e2e_tests():
    data_dir = tempfile.mkdtemp(prefix="kamish-e2e-data-")
    print(f"[*] Starting kamish-gateway on port {GATEWAY_PORT} with data dir: {data_dir}")

    gw_proc = subprocess.Popen(
        ["target/debug/kamish-gateway", "--port", str(GATEWAY_PORT), "--data-dir", data_dir],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    print(f"[*] Starting vite preview on port {FRONTEND_PORT}")
    fe_proc = subprocess.Popen(
        ["npx", "vite", "preview", "--port", str(FRONTEND_PORT), "--strictPort"],
        cwd="frontend",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    try:
        if not wait_for_service(f"{GATEWAY_URL}/api/status", 15):
            print("[-] Gateway failed to start!")
            sys.exit(1)
        print("[+] Gateway is listening and responding!")

        if not wait_for_service(FRONTEND_URL, 15):
            print("[-] Frontend preview failed to start!")
            sys.exit(1)
        print("[+] Frontend is listening and responding!")

        print("[*] Launching Chromium browser via Playwright...")
        with sync_playwright() as p:
            browser = p.chromium.launch(
                executable_path="/usr/bin/google-chrome",
                headless=True,
                args=["--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage"]
            )
            context = browser.new_context(viewport={"width": 1280, "height": 900})
            page = context.new_page()

            # -------------------------------------------------------------
            # TEST 1: Homepage & Accessibility
            # -------------------------------------------------------------
            print("[*] TEST 1: Homepage & Accessibility Checks...")
            page.goto(FRONTEND_URL)
            page.wait_for_selector("header")
            page.wait_for_selector("main")
            page.wait_for_selector("nav")

            title = page.title()
            print(f"    Page title: {title}")
            assert "Kamish" in title, f"Title {title} does not contain Kamish"

            # Check semantic and ARIA navigation
            nav_buttons = page.locator("nav button")
            assert nav_buttons.count() >= 5, "Navigation dock missing buttons"

            # Check keyboard accessibility: tab through navigation
            page.keyboard.press("Tab")
            focused_tag = page.evaluate("() => document.activeElement.tagName")
            assert focused_tag in ("BUTTON", "A"), "First tab stop should be interactive element"
            print("    [✓] Semantic HTML & Accessibility verified!")

            # -------------------------------------------------------------
            # TEST 2: LIVE vs DEMO Mode Toggle
            # -------------------------------------------------------------
            print("[*] TEST 2: LIVE vs DEMO Mode Toggle...")
            badge = page.locator("#mode-badge")
            assert "LIVE BROKER" in badge.inner_text(), "Initial mode should be LIVE BROKER"

            toggle_btn = page.locator("#mode-toggle-btn")
            toggle_btn.click()
            page.wait_for_timeout(300)
            assert "DEMO / SIMULATION" in badge.inner_text(), "Mode should update to DEMO"

            toggle_btn.click()
            page.wait_for_timeout(300)
            assert "LIVE BROKER" in badge.inner_text(), "Mode should toggle back to LIVE"
            print("    [✓] LIVE vs DEMO toggle verified!")

            # -------------------------------------------------------------
            # TEST 3: Authentication & Real Login
            # -------------------------------------------------------------
            print("[*] TEST 3: Authentication & Session...")
            page.click("#nav-signin-btn")
            page.wait_for_selector("#username")

            # A. Invalid login attempt
            page.fill("#username", "admin")
            page.fill("#password", "wrong-password")
            page.click("#signin-btn")
            page.wait_for_selector('[role="alert"]', timeout=5000)
            error_text = page.locator('[role="alert"]').inner_text()
            print(f"    Bad credentials rejected: {error_text}")

            # B. Valid login attempt
            page.fill("#username", "admin")
            page.fill("#password", "kamish-admin-2026")
            page.click("#signin-btn")

            # Check user badge in header
            page.wait_for_selector("#user-badge", timeout=5000)
            user_badge_text = page.locator("#user-badge").inner_text()
            print(f"    User badge in header: {user_badge_text}")
            assert "admin" in user_badge_text.lower(), "User badge missing username"
            assert "admin" in user_badge_text.lower(), "User badge missing role"
            print("    [✓] Authentication & session creation verified!")

            # -------------------------------------------------------------
            # TEST 4: Topic Catalog Management
            # -------------------------------------------------------------
            print("[*] TEST 4: Topic Catalog Management...")
            page.click("#nav-topics")
            page.wait_for_selector("#topic-name", timeout=5000)

            # Check pre-registered topics
            page.wait_for_selector("text=financial-transactions", timeout=5000)
            page.wait_for_selector("text=sovereign-stream", timeout=5000)

            # Create new topic
            test_topic = f"settlement-{int(time.time())}"
            page.fill("#topic-name", test_topic)
            page.fill("#part-count", "3")
            page.select_option("#class-select", "sovereign-class")
            page.click('button:has-text("REGISTER TOPIC")')

            page.wait_for_selector(f"text={test_topic}", timeout=5000)
            print(f"    [✓] Created new topic {test_topic} in catalog!")

            # -------------------------------------------------------------
            # TEST 5: Real Produce & Fetch End-to-End
            # -------------------------------------------------------------
            print("[*] TEST 5: Real Produce & Fetch Console...")
            page.click("#nav-console")
            page.wait_for_selector("#prod-topic", timeout=5000)

            test_payload = f"E2E_PAYLOAD_TOKEN_{int(time.time())}"
            page.select_option("#prod-topic", "financial-transactions")
            page.fill("#prod-part", "0")
            page.select_option("#prod-class", "sovereign-class")
            page.fill("#prod-payload", test_payload)
            page.click("#submit-produce-btn")

            # Wait for ACK
            page.wait_for_selector("text=ACKNOWLEDGED BY BROKER", timeout=5000)
            print("    [✓] Broker acknowledged produce operation!")

            # Fetch record
            page.select_option("#fetch-topic", "financial-transactions")
            page.fill("#fetch-part", "0")
            page.fill("#fetch-offset", "0")
            page.click("#submit-fetch-btn")

            page.wait_for_selector(f"text={test_payload}", timeout=5000)
            print(f"    [✓] Fetch verified: record payload '{test_payload}' retrieved from broker!")

            # -------------------------------------------------------------
            # TEST 6: Metadata Authority & Policy Version Advance
            # -------------------------------------------------------------
            print("[*] TEST 6: Authority & Policy Advance...")
            page.click("#nav-authority")
            page.wait_for_selector("#policy-ver", timeout=5000)

            page.fill("#policy-ver", "3")
            page.click('button:has-text("PUBLISH POLICY UPDATE")')
            page.wait_for_selector("text=advanced to 3", timeout=5000)
            print("    [✓] Signed policy version floor advanced to v3!")

            # -------------------------------------------------------------
            # TEST 7: Audit Verification & Checkers
            # -------------------------------------------------------------
            print("[*] TEST 7: Cryptographic Audit Verification...")
            page.click("#nav-audit")
            page.wait_for_selector("#verify-audit-btn", timeout=5000)

            page.click("#verify-audit-btn")
            page.wait_for_selector("#verify-result-box", timeout=5000)
            verify_text = page.locator("#verify-result-box").inner_text()
            assert "CRYPTOGRAPHIC AUDIT VERIFIED" in verify_text, "Verification text mismatch"
            assert "ZERO (INTEGRITY INTACT)" in verify_text, "Tamper detected"
            print("    [✓] Real cryptographic audit verification PASSED!")

            # -------------------------------------------------------------
            # TEST 8: Logout & Session Expiry
            # -------------------------------------------------------------
            print("[*] TEST 8: Logout & Session Invalidation...")
            page.click("#logout-btn")
            page.wait_for_timeout(300)
            assert page.locator("#user-badge").count() == 0, "User badge should disappear on logout"
            assert page.locator("#nav-signin-btn").count() == 1, "Sign in button should reappear"
            print("    [✓] Logout & session cleanup verified!")

            browser.close()
            print("\n[SUCCESS] ALL 8 END-TO-END PLAYWRIGHT BROWSER TESTS PASSED!")

    finally:
        print("[*] Cleaning up processes...")
        try:
            gw_proc.terminate()
            gw_proc.wait(timeout=3)
        except Exception:
            gw_proc.kill()

        try:
            fe_proc.terminate()
            fe_proc.wait(timeout=3)
        except Exception:
            fe_proc.kill()

        shutil.rmtree(data_dir, ignore_errors=True)

if __name__ == "__main__":
    run_e2e_tests()
