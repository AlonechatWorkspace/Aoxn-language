// Cross-platform functional test for the web suite.
//
//   node parity.mjs <path-to-aoxn-server> [port]
//
// Verifies: (1) the Aoxn server's /, /api/json, /text bodies are
// byte-identical to the Node reference server, (2) 404 handling,
// (3) keep-alive serves sequential requests, (4) /metrics answers in
// Prometheus text format and counts the traffic.

import { spawn, execFileSync } from "node:child_process";
import path from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const webDir = path.resolve(__dirname, "..");
const aoxnBin = process.argv[2]
  ? path.resolve(process.argv[2])
  : path.join(webDir, process.platform === "win32" ? "server.exe" : "server");
const PORT = Number(process.argv[3] ?? 3000);
const NODE_PORT = PORT + 1;

let failures = 0;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? " - " + detail : ""}`);
  if (!ok) failures++;
}

async function waitReady(url) {
  for (let i = 0; i < 100; i++) {
    try {
      const r = await fetch(url);
      if (r.ok) return;
    } catch {
      // not up yet
    }
    await sleep(100);
  }
  throw new Error(`server never became ready: ${url}`);
}

function killTree(child) {
  try {
    if (process.platform === "win32") {
      execFileSync("taskkill", ["/T", "/F", "/PID", String(child.pid)], { stdio: "ignore" });
    } else {
      process.kill(-child.pid, "SIGKILL");
    }
  } catch {
    // already gone
  }
}

const aoxn = spawn(aoxnBin, [], {
  cwd: webDir,
  stdio: ["ignore", "ignore", "inherit"],
  detached: process.platform !== "win32",
});
const nodeRef = spawn(process.execPath, [path.join(webDir, "node-server.mjs")], {
  cwd: webDir,
  env: { ...process.env, PORT: String(NODE_PORT) },
  stdio: ["ignore", "ignore", "inherit"],
  detached: process.platform !== "win32",
});

try {
  await waitReady(`http://127.0.0.1:${PORT}/text`);
  await waitReady(`http://127.0.0.1:${NODE_PORT}/text`);

  for (const route of ["/text", "/api/json", "/"]) {
    const a = Buffer.from(await (await fetch(`http://127.0.0.1:${PORT}${route}`)).arrayBuffer());
    const b = Buffer.from(await (await fetch(`http://127.0.0.1:${NODE_PORT}${route}`)).arrayBuffer());
    check(`body parity ${route}`, a.equals(b), `${a.length} vs ${b.length} bytes`);
  }

  const nf = await fetch(`http://127.0.0.1:${PORT}/nope`);
  const nfBody = await nf.text();
  check("404 for unknown route", nf.status === 404 && nfBody === "not found\n");

  let keepAliveOk = true;
  for (let i = 0; i < 3; i++) {
    const r = await fetch(`http://127.0.0.1:${PORT}/text`);
    if (!r.ok || (await r.text()) !== "hello, web\n") keepAliveOk = false;
  }
  check("keep-alive sequential x3", keepAliveOk);

  const m = await (await fetch(`http://127.0.0.1:${PORT}/metrics`)).text();
  check(
    "metrics prometheus format",
    m.includes("# TYPE aoxn_http_requests_total counter") &&
      m.includes("aoxn_http_uptime_seconds") &&
      m.includes("aoxn_http_connections_total")
  );
  check("metrics counts this test", /aoxn_http_requests_total\{route="\/text"\} [1-9]/.test(m));
  check("metrics counts 404s", /aoxn_http_not_found_total [1-9]/.test(m));
} catch (err) {
  check("suite ran to completion", false, String(err));
} finally {
  killTree(aoxn);
  killTree(nodeRef);
}

console.log(failures === 0 ? "\nparity: ALL PASS" : `\nparity: ${failures} FAILURE(S)`);
process.exit(failures === 0 ? 0 : 1);
