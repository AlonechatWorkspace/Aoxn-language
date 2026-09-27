// Load-test orchestrator for the Aoxn vs pnpm+nodejs+nextjs web benchmark.
//
//   pnpm install && pnpm bench        (from web/loadtest)
//
// Engine: oha (https://github.com/hatoo/oha) — tools/oha.exe on Windows,
// see README.md for the download. Each target is started in turn on
// 127.0.0.1:3000, warmed up, then every route is load-tested. The whole
// matrix runs TRIALS times interleaved per target and the median run is
// reported (this dev machine's timings swing ~2x with AV/indexing noise).

import { spawn, execFileSync } from "node:child_process";
import fs from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const webDir = path.resolve(__dirname, "..");
const PORT = Number(process.env.BENCH_PORT ?? 3000);
const BASE = `http://127.0.0.1:${PORT}`;
const DURATION = process.env.BENCH_DURATION ?? "10s";
const CONNECTIONS = Number(process.env.BENCH_CONNECTIONS ?? 32);
const TRIALS = Number(process.env.BENCH_TRIALS ?? 3);
const ROUTES = ["/text", "/api/json", "/"];
const OHA = path.join(__dirname, "tools", process.platform === "win32" ? "oha.exe" : "oha");

const TARGETS = [
  {
    name: "Aoxn v0.26.3 (aoxn build, O3)",
    cmd: path.join(webDir, process.platform === "win32" ? "server.exe" : "server"),
    args: [],
    cwd: webDir,
  },
  {
    name: "Node.js 22 (node:http)",
    cmd: process.execPath,
    args: [path.join(webDir, "node-server.mjs")],
    cwd: webDir,
  },
  {
    name: "Next.js 15 (next start)",
    cmd: process.execPath,
    args: [path.join(webDir, "next-app", "node_modules", "next", "dist", "bin", "next"), "start", "-p", String(PORT)],
    cwd: path.join(webDir, "next-app"),
    env: { NEXT_TELEMETRY_DISABLED: "1" },
  },
];

function oha(args, parseJson = true) {
  return new Promise((resolve, reject) => {
    const child = spawn(OHA, args, { stdio: ["ignore", "pipe", "pipe"] });
    let out = "";
    let err = "";
    child.stdout.on("data", (d) => (out += d));
    child.stderr.on("data", (d) => (err += d));
    child.on("error", reject);
    child.on("close", () => {
      if (!parseJson) {
        resolve(null);
        return;
      }
      try {
        resolve(JSON.parse(out));
      } catch (e) {
        reject(new Error(`oha output not JSON: ${out.slice(0, 200)} | stderr: ${err.slice(0, 400)}`));
      }
    });
  });
}

function load(url, duration = DURATION) {
  return oha(["-z", duration, "-c", String(CONNECTIONS), "-w", "--no-tui", "--output-format", "json", url]);
}

function warmup(url) {
  return oha(["-z", "3s", "-c", String(CONNECTIONS), "--no-tui", "--output-format", "quiet", url], false);
}

function sampleRss(pid) {
  try {
    if (process.platform === "win32") {
      const out = execFileSync(
        "powershell",
        ["-NoProfile", "-Command", `(Get-Process -Id ${pid}).WorkingSet64`],
        { encoding: "utf8" }
      );
      return Number(out.trim()) / 1024 / 1024;
    }
    try {
      const status = fs.readFileSync(`/proc/${pid}/status`, "utf8");
      const m = status.match(/VmRSS:\s+(\d+) kB/);
      if (m) return Number(m[1]) / 1024;
    } catch {
      // no procfs (macOS) - fall through to ps
    }
    const out = execFileSync("ps", ["-o", "rss=", "-p", String(pid)], { encoding: "utf8" });
    return Number(out.trim()) / 1024;
  } catch {
    return NaN;
  }
}

// wait until nothing answers on the benchmark port (a leaked server from a
// previous test step would otherwise make every target "exit early")
async function waitPortFree(port, ms = 5000) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    try {
      await fetch(`http://127.0.0.1:${port}/text`, { signal: AbortSignal.timeout(400) });
    } catch {
      return true;
    }
    await sleep(200);
  }
  return false;
}

async function waitReady(child, startedAt, getSpawnError = () => null) {
  for (let i = 0; i < 600; i++) {
    const spawnError = getSpawnError();
    if (spawnError) throw new Error(`server spawn failed: ${spawnError.message}`);
    if (child.exitCode !== null) throw new Error(`server exited early (code ${child.exitCode})`);
    try {
      const res = await fetch(`${BASE}/text`);
      if (res.ok) {
        await res.text();
        return Date.now() - startedAt;
      }
    } catch {
      // not up yet
    }
    await sleep(100);
  }
  throw new Error("server never became ready");
}

function killServer(child) {
  try {
    if (process.platform === "win32") {
      execFileSync("taskkill", ["/T", "/F", "/PID", String(child.pid)], { stdio: "ignore" });
    } else {
      try {
        process.kill(-child.pid, "SIGKILL");
      } catch {
        child.kill("SIGKILL");
      }
    }
  } catch {
    // already gone
  }
}

function median(xs) {
  const sorted = [...xs].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
}

const runs = []; // { trial, target, route, rps, p50, p95, p99, meanLat, errors, rssMb }
const readyMap = new Map();
let hadFailure = false;

for (let trial = 1; trial <= TRIALS; trial++) {
  for (const target of TARGETS) {
    console.log(`\n=== trial ${trial}/${TRIALS} — ${target.name} ===`);
    if (!(await waitPortFree(PORT))) {
      hadFailure = true;
      console.error(`FAILED: port ${PORT} still occupied before starting ${target.name} (leaked server?)`);
      continue;
    }
    const startedAt = Date.now();
    let serverStderr = "";
    const child = spawn(target.cmd, target.args, {
      cwd: target.cwd,
      stdio: ["ignore", "ignore", "pipe"],
      env: { ...process.env, ...(target.env ?? {}) },
      detached: process.platform !== "win32",
    });
    child.stderr.on("data", (d) => (serverStderr += d.toString()));
    let spawnError = null;
    child.on("error", (err) => (spawnError = err));
    try {
      const readyMs = await waitReady(child, startedAt, () => spawnError);
      if (!readyMap.has(target.name)) readyMap.set(target.name, readyMs);
      console.log(`ready in ${readyMs} ms`);
      await warmup(`${BASE}/text`);
      await sleep(300);

      for (const route of ROUTES) {
        let rss = NaN;
        const sampler = setInterval(() => {
          rss = sampleRss(child.pid);
        }, 2000);
        const r = await load(`${BASE}${route}`);
        clearInterval(sampler);
        const lat = r.metrics.latency_ms;
        const errors = Object.values(r.errorDistribution ?? {}).reduce((a, b) => a + b, 0);
        runs.push({
          trial,
          target: target.name,
          route,
          rps: r.metrics.requests_per_sec,
          meanLat: lat.mean,
          p50: lat.p50,
          p95: lat.p95,
          p99: lat.p99,
          maxLat: lat.max,
          errors,
          rssMb: rss,
        });
        console.log(
          `${route.padEnd(10)} ${Math.round(r.metrics.requests_per_sec).toString().padStart(8)} req/s  ` +
            `p50 ${lat.p50.toFixed(3)} ms  p95 ${lat.p95.toFixed(3)} ms  p99 ${lat.p99.toFixed(3)} ms  errors ${errors}  rss ${rss.toFixed(0)} MB`
        );
        await sleep(300);
      }
    } catch (err) {
      hadFailure = true;
      console.error(`FAILED: ${err.message}`);
      if (serverStderr.trim()) console.error(`--- ${target.name} stderr ---\n${serverStderr.slice(-2000)}`);
    } finally {
      killServer(child);
      await sleep(500);
    }
  }
}

// aggregate: median run per target+route
const aggregate = [];
for (const target of TARGETS) {
  for (const route of ROUTES) {
    const rs = runs.filter((x) => x.target === target.name && x.route === route);
    if (rs.length === 0) continue;
    aggregate.push({
      target: target.name,
      route,
      rps: median(rs.map((x) => x.rps)),
      p50: median(rs.map((x) => x.p50)),
      p95: median(rs.map((x) => x.p95)),
      p99: median(rs.map((x) => x.p99)),
      errors: rs.reduce((a, x) => a + x.errors, 0),
      rssMb: median(rs.map((x) => x.rssMb)),
      trials: rs.length,
    });
  }
}

console.log("\n\n=== MEDIAN OVER TRIALS ===");
for (const a of aggregate) {
  console.log(
    `${a.target.padEnd(32)} ${a.route.padEnd(10)} ${Math.round(a.rps).toString().padStart(8)} req/s  ` +
      `p50 ${a.p50.toFixed(3)} ms  p95 ${a.p95.toFixed(3)} ms  p99 ${a.p99.toFixed(3)} ms  errors ${a.errors}  rss ${a.rssMb.toFixed(0)} MB`
  );
}

const outPath = path.join(__dirname, "last-results.json");
fs.writeFileSync(
  outPath,
  JSON.stringify({ duration: DURATION, connections: CONNECTIONS, trials: TRIALS, readyMs: Object.fromEntries(readyMap), runs, aggregate }, null, 2)
);
console.log(`\nresults written to ${outPath}`);

if (process.env.GITHUB_STEP_SUMMARY) {
  const rows = aggregate
    .map(
      (a) =>
        `| ${a.target} | ${a.route} | ${Math.round(a.rps)} | ${a.p50.toFixed(3)} | ${a.p95.toFixed(3)} | ${a.p99.toFixed(3)} | ${a.errors} | ${a.rssMb.toFixed(0)} |`
    )
    .join("\n");
  fs.appendFileSync(
    process.env.GITHUB_STEP_SUMMARY,
    `## Web reference benchmark (${DURATION}, ${CONNECTIONS} connections, ${TRIALS} trial(s))\n\n` +
      `| Server | Route | req/s | p50 ms | p95 ms | p99 ms | errors | RSS MB |\n` +
      `|---|---|---:|---:|---:|---:|---:|---:|\n${rows}\n\n` +
      `CI runners share CPUs - reference/trend values only.\n`
  );
}

if (hadFailure) process.exitCode = 1;
