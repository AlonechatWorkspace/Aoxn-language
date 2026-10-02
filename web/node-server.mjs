// Plain Node.js (node:http) server — the "nodejs" leg of the
// pnpm+nodejs+nextjs benchmark stack. Renders byte-identical bodies to the
// Aoxn server (web/http_buf.ax); routes: /, /api/json, /text, plus the
// static file service with the SAME semantics (ETag, If-None-Match → 304,
// single-range → 206, unsatisfiable → 416, Cache-Control) — parity.mjs
// cross-checks both servers.

import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const webDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)));

// ---- static files: mirror of http_buf.ax's FileTable ----

const STATIC_NAMES = ["index.html", "style.css", "app.js", "logo.svg"];

function mimeOf(name) {
  if (name.endsWith(".html")) return "text/html; charset=utf-8";
  if (name.endsWith(".css")) return "text/css; charset=utf-8";
  if (name.endsWith(".js")) return "text/javascript; charset=utf-8";
  if (name.endsWith(".svg")) return "image/svg+xml";
  if (name.endsWith(".json")) return "application/json";
  if (name.endsWith(".txt")) return "text/plain; charset=utf-8";
  return "application/octet-stream";
}

// identical arithmetic to content_hash in http_buf.ax (bounded, no
// 64-bit wraparound) so the ETags match byte for byte
function contentHash(buf) {
  let h = 0;
  for (let i = 0; i < buf.length; i++) {
    h = (h + buf[i] * ((i % 127) + 1)) % 1000000007;
  }
  return h;
}

const staticFiles = new Map();
for (const name of STATIC_NAMES) {
  try {
    const buf = readFileSync(path.join(webDir, "static", name));
    staticFiles.set(name, {
      buf,
      etag: `"${buf.length}.${contentHash(buf)}"`,
      mime: mimeOf(name),
    });
  } catch {
    // missing file: stays absent — requests 404, like an empty Aoxn entry
  }
}

// parse a single-range Range header exactly like parse_range in
// http_buf.ax: kind 0 = ignore (200), 1 = valid (206), 2 = unsatisfiable (416)
function parseRange(header, total) {
  if (header === undefined) return { kind: 0, lo: 0, hi: 0 };
  const m = /^bytes=(\d*)-(\d*)$/.exec(header);
  if (!m) return { kind: 0, lo: 0, hi: 0 };
  const [, a, b] = m;
  if (a === "" && b === "") return { kind: 0, lo: 0, hi: 0 };
  if (a === "") {
    const n = Number(b);
    if (n === 0) return { kind: 0, lo: 0, hi: 0 };
    if (total === 0) return { kind: 2, lo: 0, hi: 0 };
    return { kind: 1, lo: Math.max(0, total - n), hi: total - 1 };
  }
  const lo = Number(a);
  if (b === "") {
    if (lo >= total) return { kind: 2, lo: 0, hi: 0 };
    return { kind: 1, lo, hi: total - 1 };
  }
  const hi = Number(b);
  if (lo > hi) return { kind: 0, lo: 0, hi: 0 };
  if (lo >= total) return { kind: 2, lo: 0, hi: 0 };
  return { kind: 1, lo, hi: Math.min(hi, total - 1) };
}

const CACHE_CONTROL = "public, max-age=60";

function serveStatic(req, res, f) {
  const total = f.buf.length;
  if (req.headers["if-none-match"] === f.etag || req.headers["if-none-match"] === "*") {
    res.writeHead(304, { ETag: f.etag, "Cache-Control": CACHE_CONTROL });
    res.end();
    return;
  }
  const range = parseRange(req.headers.range, total);
  if (range.kind === 1) {
    res.writeHead(206, {
      "Content-Type": f.mime,
      ETag: f.etag,
      "Cache-Control": CACHE_CONTROL,
      "Content-Range": `bytes ${range.lo}-${range.hi}/${total}`,
      "Content-Length": range.hi - range.lo + 1,
      "Connection": "keep-alive",
    });
    res.end(f.buf.subarray(range.lo, range.hi + 1));
  } else if (range.kind === 2) {
    const body = "byte range not satisfiable\n";
    res.writeHead(416, {
      "Content-Type": f.mime,
      ETag: f.etag,
      "Cache-Control": CACHE_CONTROL,
      "Content-Range": `bytes */${total}`,
      "Content-Length": body.length,
      "Connection": "keep-alive",
    });
    res.end(body);
  } else {
    res.writeHead(200, {
      "Content-Type": f.mime,
      ETag: f.etag,
      "Cache-Control": CACHE_CONTROL,
      "Content-Length": total,
      "Connection": "keep-alive",
    });
    res.end(f.buf);
  }
}

function bodyHtml() {
  let rows = "";
  let total = 0;
  for (let i = 1; i <= 20; i++) {
    const sq = i * i;
    total += sq;
    rows += `<tr><td>${i}</td><td>${sq}</td></tr>\n`;
  }
  return (
    "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>Aoxn Web Benchmark</title>\n</head>\n<body>\n" +
    "<h1>Aoxn Web Benchmark</h1>\n<p>Server-side rendered for the Aoxn web benchmark.</p>\n" +
    "<table>\n<tr><th>n</th><th>n*n</th></tr>\n" +
    rows +
    "</table>\n<p>Sum of squares 1..20: " + total + "</p>\n</body>\n</html>\n"
  );
}

function bodyJson() {
  const squares = [];
  let total = 0;
  for (let i = 1; i <= 20; i++) {
    const sq = i * i;
    squares.push(sq);
    total += sq;
  }
  return JSON.stringify({ language: "Aoxn", version: "0.26.3", squares, sum: total });
}

const server = createServer((req, res) => {
  const rawTarget = (req.url ?? "/").split("?")[0];
  if (req.method === "GET" && rawTarget.startsWith("/static/")) {
    const name = rawTarget.slice("/static/".length);
    const f = staticFiles.get(name);
    if (f) {
      serveStatic(req, res, f);
      return;
    }
  }
  if (req.url === "/") {
    res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
    res.end(bodyHtml());
  } else if (req.url === "/api/json") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(bodyJson());
  } else if (req.url === "/text") {
    res.writeHead(200, { "Content-Type": "text/plain; charset=utf-8" });
    res.end("hello, web\n");
  } else {
    res.writeHead(404, { "Content-Type": "text/plain; charset=utf-8" });
    res.end("not found\n");
  }
});

const port = Number(process.env.PORT ?? 3000);
server.listen(port, "127.0.0.1", () => {
  console.log("Node.js web server listening on port " + port);
});
