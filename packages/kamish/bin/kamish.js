#!/usr/bin/env node

/**
 * Kamish CLI Executable Wrapper (Phase 6 / ADR-006).
 * Resolves the platform-native Rust binary when available, and seamlessly
 * falls back to the cross-platform portable engine on Windows, macOS, and Linux.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const isWindows = process.platform === "win32";
const binaryExeName = isWindows ? "kamish.exe" : "kamish";

const PLATFORM_PACKAGES = {
  "linux-x64": { pkg: "kamish-linux-x64", bin: "kamish" },
  "linux-arm64": { pkg: "kamish-linux-arm64", bin: "kamish" },
  "darwin-x64": { pkg: "kamish-darwin-x64", bin: "kamish" },
  "darwin-arm64": { pkg: "kamish-darwin-arm64", bin: "kamish" },
  "win32-x64": { pkg: "kamish-win32-x64", bin: "kamish.exe" },
  "win32-arm64": { pkg: "kamish-win32-x64", bin: "kamish.exe" },
};

function isValidBinary(filePath) {
  try {
    if (!fs.existsSync(filePath)) return false;
    const stat = fs.statSync(filePath);
    if (!stat.isFile() || stat.size < 1024) return false;
    // Inspect magic bytes to distinguish real compiled binaries from shell/batch script fallbacks
    const fd = fs.openSync(filePath, "r");
    const buf = Buffer.alloc(4);
    fs.readSync(fd, buf, 0, 4, 0);
    fs.closeSync(fd);
    // Shebang (#! / 0x23 0x21) -> shell script fallback
    if (buf[0] === 0x23 && buf[1] === 0x21) return false;
    // @ECH (0x40 0x45 0x43 0x48) -> batch file fallback
    if (buf[0] === 0x40 && buf[1] === 0x45 && buf[2] === 0x43 && buf[3] === 0x48) return false;
    return true;
  } catch {
    return false;
  }
}

function resolveBinary() {
  // Allow explicitly forcing the portable runtime engine
  if (process.env.KAMISH_PORTABLE === "1" || process.env.KAMISH_FORCE_PORTABLE === "1") {
    return null;
  }
  // 1. Explicit override via environment variable
  if (process.env.KAMISH_BINARY_PATH) {
    if (isValidBinary(process.env.KAMISH_BINARY_PATH)) {
      return process.env.KAMISH_BINARY_PATH;
    }
    console.warn(
      `[kamish] Warning: Specified KAMISH_BINARY_PATH does not exist or is invalid: ${process.env.KAMISH_BINARY_PATH}`
    );
  }

  const platformKey = `${process.platform}-${process.arch}`;
  const spec = PLATFORM_PACKAGES[platformKey];

  if (spec) {
    const candidateBins = [spec.bin, `${spec.bin}-native`, `${spec.bin}.bin`];
    // 2. Try resolving via Node module lookup (optionalDependencies)
    for (const b of candidateBins) {
      try {
        const resolvedPath = require.resolve(`${spec.pkg}/bin/${b}`);
        if (isValidBinary(resolvedPath)) {
          return resolvedPath;
        }
      } catch {
        // Fall through
      }
    }

    // 3. Search node_modules tree upwards
    let currentDir = __dirname;
    while (true) {
      for (const b of candidateBins) {
        const candidate = path.join(currentDir, "node_modules", spec.pkg, "bin", b);
        if (isValidBinary(candidate)) {
          return candidate;
        }
        const siblingCandidate = path.join(path.dirname(currentDir), spec.pkg, "bin", b);
        if (isValidBinary(siblingCandidate)) {
          return siblingCandidate;
        }
      }
      const parent = path.dirname(currentDir);
      if (parent === currentDir) break;
      currentDir = parent;
    }
  }

  // 4. Local workspace target fallback (for development and repo builds)
  const repoTargets = [
    path.resolve(__dirname, `../../../target/release/${binaryExeName}`),
    path.resolve(__dirname, `../../../target/debug/${binaryExeName}`),
    path.resolve(__dirname, `../../target/release/${binaryExeName}`),
    path.resolve(__dirname, `../../target/debug/${binaryExeName}`),
  ];
  for (const t of repoTargets) {
    if (isValidBinary(t)) {
      return t;
    }
  }

  return null;
}

const binaryPath = resolveBinary();

if (binaryPath) {
  // Execute native binary
  try {
    const result = spawnSync(binaryPath, process.argv.slice(2), {
      stdio: "inherit",
      env: process.env,
      windowsHide: true,
    });

    if (result.error) {
      console.error(`[kamish] Failed to execute native binary: ${result.error.message}`);
      process.exit(1);
    }

    process.exit(result.status ?? (result.signal ? 1 : 0));
  } catch (err) {
    console.error(`[kamish] Execution error: ${err.message}`);
    process.exit(1);
  }
} else {
  // Cross-Platform Portable Engine Fallback (Windows, macOS, and Linux without native binary)
  const { runPortableCli } = await import("../lib/portable-runner.js");
  runPortableCli(process.argv.slice(2)).catch(err => {
    console.error(`[kamish] Runtime error: ${err.message}`);
    process.exit(1);
  });
}
