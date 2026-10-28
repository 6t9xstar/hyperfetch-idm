#!/usr/bin/env node
import { spawn } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { performance } from "node:perf_hooks";

function parseArgs(argv) {
  const options = {
    sizeMiB: 256,
    runs: 5,
    warmup: 1,
    connections: [8],
    batches: [1024 * 1024],
    binary: process.platform === "win32" ? "target/release/hyperfetch.exe" : "target/release/hyperfetch",
    fixture: "http://127.0.0.1:8080",
    url: null,
    json: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const value = argv[index + 1];
    switch (arg) {
      case "--size":
        options.sizeMiB = Number(value);
        index += 1;
        break;
      case "--runs":
        options.runs = Number(value);
        index += 1;
        break;
      case "--warmup":
        options.warmup = Number(value);
        index += 1;
        break;
      case "--connections":
        options.connections = value.split(",").map(Number);
        index += 1;
        break;
      case "--batch":
        options.batches = value.split(",").map(Number);
        index += 1;
        break;
      case "--binary":
        options.binary = value;
        index += 1;
        break;
      case "--fixture":
        options.fixture = value;
        index += 1;
        break;
      case "--url":
        options.url = value;
        index += 1;
        break;
      case "--json":
        options.json = true;
        break;
      case "--help":
        console.log(
          "Usage: node scripts/bench.mjs [--size 256] [--runs 5] [--warmup 1] " +
            "[--connections 4,8,16] [--batch 262144,1048576,4194304] " +
            "[--binary path] [--fixture http://127.0.0.1:8080] [--url URL] [--json]",
        );
        process.exit(0);
        break;
      default:
        throw new Error(`Unknown option: ${arg}`);
    }
  }

  if (!Number.isFinite(options.sizeMiB) || options.sizeMiB <= 0) {
    throw new Error("--size must be a positive number of MiB");
  }
  if (!Number.isInteger(options.runs) || options.runs < 1) {
    throw new Error("--runs must be at least 1");
  }
  if (!Number.isInteger(options.warmup) || options.warmup < 0) {
    throw new Error("--warmup must be 0 or more");
  }
  for (const value of [...options.connections, ...options.batches]) {
    if (!Number.isFinite(value) || value <= 0) {
      throw new Error("--connections and --batch must be positive numbers");
    }
  }
  return options;
}

function runProcess(binary, args, cwd) {
  return new Promise((resolve, reject) => {
    const child = spawn(binary, args, { cwd, windowsHide: true });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString();
    });
    child.on("error", reject);
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });
}

async function resetStats(fixture) {
  try {
    await fetch(new URL("/stats/reset", fixture));
  } catch {
    return false;
  }
  return true;
}

async function readStats(fixture) {
  try {
    const response = await fetch(new URL("/stats", fixture));
    if (!response.ok) return null;
    return await response.json();
  } catch {
    return null;
  }
}

function summarize(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const sum = sorted.reduce((total, value) => total + value, 0);
  const middle = Math.floor(sorted.length / 2);
  const median =
    sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
  return {
    min: sorted[0],
    median,
    mean: sum / sorted.length,
    max: sorted[sorted.length - 1],
  };
}

function formatMiB(value) {
  return value.toFixed(1).padStart(7);
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const root = process.cwd();
  const binary = path.resolve(root, options.binary);
  await fs.access(binary).catch(() => {
    throw new Error(
      `Binary not found at ${binary}. Run: cargo build --release -p hyperfetch-cli`,
    );
  });

  const bytes = Math.round(options.sizeMiB * 1024 * 1024);
  const url = options.url ?? `${options.fixture}/stream/${bytes}`;
  const tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), "hyperfetch-bench-"));
  const canUseStats = await resetStats(options.fixture);
  const results = [];

  try {
    for (const connections of options.connections) {
      for (const batch of options.batches) {
        const runs = [];
        for (let index = 0; index < options.warmup + options.runs; index += 1) {
          const directory = path.join(tempRoot, `${connections}-${batch}-${index}`);
          const db = path.join(tempRoot, `bench-${connections}-${batch}-${index}.db`);
          await resetStats(options.fixture);
          const started = performance.now();
          const { code, stdout, stderr } = await runProcess(
            binary,
            [
              "--db",
              db,
              "download",
              url,
              "--dir",
              directory,
              "--connections",
              String(connections),
              "--batch-size",
              String(batch),
              "--no-hash",
              "--quiet",
            ],
            root,
          );
          const elapsed = (performance.now() - started) / 1000;
          if (code !== 0) {
            throw new Error(
              `Download failed with exit code ${code}: ${stderr.trim() || stdout.trim()}`,
            );
          }
          const done = stdout.match(/done: .*?\((\d+) bytes\)/);
          if (!done) {
            throw new Error(`Could not parse download result: ${stdout.trim()}`);
          }
          const stats = canUseStats ? await readStats(options.fixture) : null;
          const received = Number(done[1]);
          const ok = received === bytes && (!stats || stats.bytes === bytes);
          if (index >= options.warmup) {
            runs.push({ seconds: elapsed, ok, stats });
          }
          await fs.rm(directory, { recursive: true, force: true }).catch(() => {});
          await fs.rm(db, { force: true }).catch(() => {});
          await fs.rm(`${db}-wal`, { force: true }).catch(() => {});
          await fs.rm(`${db}-shm`, { force: true }).catch(() => {});
        }

        const seconds = runs.map((run) => run.seconds);
        const summary = summarize(seconds);
        const throughput = summarize(seconds.map((value) => bytes / value / (1024 * 1024)));
        const verified = runs.every((run) => run.ok);
        results.push({
          connections,
          batch,
          bytes,
          verified,
          seconds: summary,
          throughput_mib_s: throughput,
          stats: runs[runs.length - 1]?.stats ?? null,
        });

        console.log(
          `${String(options.sizeMiB).padStart(5)} MiB | ${String(connections).padStart(2)} conn | ` +
            `${(batch / 1024 / 1024).toFixed(1).padStart(5)} MiB | ` +
            `min ${formatMiB(throughput.min)} | median ${formatMiB(throughput.median)} | ` +
            `mean ${formatMiB(throughput.mean)} | max ${formatMiB(throughput.max)} MiB/s | ` +
            `${verified ? "verified" : "MISMATCH"}`,
        );
      }
    }
  } finally {
    await fs.rm(tempRoot, { recursive: true, force: true }).catch(() => {});
  }

  if (options.json) {
    console.log(JSON.stringify({ url, size_bytes: bytes, results }, null, 2));
  }
}

main().catch((error) => {
  console.error(`benchmark failed: ${error.message}`);
  process.exit(1);
});
