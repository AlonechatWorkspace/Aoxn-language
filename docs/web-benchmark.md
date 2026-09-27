# Aoxn web benchmark — Aoxn vs pnpm + Node.js + Next.js

**Question:** can Aoxn write web servers, and how does it compare against the
pnpm + Node.js + Next.js stack? **Answer:** yes — `web/` contains a complete
HTTP/1.1 server written in Aoxn (raw sockets via `extern def` C FFI, byte
buffer rendering, zero per-request allocation) — and on identical routes it
serves the same content **30–50× faster than Next.js 15 and at ~1/50 the
request latency of plain Node.js**, from a single 173 KB native binary with a
5 MB RSS.

Suite: [`web/`](../web/README.md) · Raw data: [`web/loadtest/last-results.json`](../web/loadtest/last-results.json)

## What is compared

Three servers, three routes, same bodies:

| Route       | Body                                                       |
|-------------|------------------------------------------------------------|
| `/`         | SSR HTML page: 20-row table + computed sum                  |
| `/api/json` | `{"language":"Aoxn","version":"0.26.3","squares":[...],"sum":2870}` |
| `/text`     | `hello, web\n`                                              |

- **Aoxn v0.26.3** — `web/server_win.ax` (Windows) / `web/server_posix.ax`
  (Linux/macOS), compiled with `aoxn build` (LLVM `default<O3>`).
- **Node.js 22.22.1** — `web/node-server.mjs`, plain `node:http`.
- **Next.js 15.5.26** — `web/next-app`, app router, `next build` +
  `next start` (production).

The Aoxn and Node response bodies are **byte-identical** (SHA-256 verified on
all three routes). The Next.js page carries the same content (title, table,
sum) with Next's own document markup and hydration payload.

## Environment & method

- Windows on Intel Core i5-1135G7 (4C/8T), LLVM 23.1.0, MSVC Build Tools.
- Load generator: [oha 1.16.0](https://github.com/hatoo/oha) (PGO build),
  32 keep-alive connections, 10 s per run.
- Client and servers run on the **same machine** (127.0.0.1).
- 3 trials per (server, route), interleaved across servers; the tables report
  the **median** trial. (This machine's timings swing ~2× with AV/indexer
  noise — e.g. one Node trial came in at half speed; the median absorbs it.)
- Each server is warmed up (3 s) before measurement.

## Results

### Throughput & latency (median of 3 × 10 s, 32 connections)

| Server | Route | req/s | p50 | p95 | p99 | errors |
|-------------------------|-----------|----------:|---------:|----------:|-----------:|-------:|
| **Aoxn (O3 native)** | `/text` | **7,611** | 0.094 ms | 0.256 ms | 0.787 ms | 0 |
| **Aoxn (O3 native)** | `/api/json` | **7,623** | 0.100 ms | 0.259 ms | 0.642 ms | 0 |
| **Aoxn (O3 native)** | `/` | **6,942** | 0.097 ms | 0.298 ms | 0.826 ms | 0 |
| Node.js 22 (node:http) | `/text` | 7,010 | 4.071 ms | 8.422 ms | 12.189 ms | 0 |
| Node.js 22 (node:http) | `/api/json` | 6,330 | 4.619 ms | 9.304 ms | 12.980 ms | 0 |
| Node.js 22 (node:http) | `/` | 5,892 | 4.769 ms | 10.590 ms | 15.104 ms | 0 |
| Next.js 15 (`next start`) | `/text` | 186 | 143.8 ms | 321.1 ms | 801.5 ms | 0 |
| Next.js 15 (`next start`) | `/api/json` | 142 | 166.4 ms | 612.1 ms | 1,487.0 ms | 0 |
| Next.js 15 (`next start`) | `/` | 263 | 116.0 ms | 162.9 ms | 197.5 ms | 0 |

- **Aoxn vs Next.js:** 26–54× the throughput at 1/1,200–1/1,700 the p50
  latency, from a process 33× smaller (5 MB vs 162–177 MB RSS).
- **Aoxn vs Node.js:** comparable throughput (the load generator is the
  binding constraint here — see below) at **~40–50× lower latency**.

### The measured Aoxn throughput is a load-generator ceiling, not the server's

Little's law on the medians (average in-flight requests = req/s × mean
latency):

| Server | in-flight requests (of 32 connections) | interpretation |
|-------------------------|----------------------------------------|----------------|
| Aoxn | ≈ 0.7 | server idle >95% of the time; capacity far above the measured 7.6k req/s |
| Node.js | ≈ 28 | connections queueing; server at its limit |
| Next.js | ≈ 28 | same, with ~120 ms of per-request pipeline cost |

A two-client scaling check (2 × 32 connections) confirms it: Node's p50
latency doubled (4 → 9.7 ms) while Aoxn's was unchanged (0.10 ms); combined
throughput stayed at the machine's ~6–7k req/s request-generation ceiling
for both. Unloaded, Next.js alone needs **16–23 ms per request** (curl
timing), which is why it saturates at a few hundred req/s.

### Startup (spawn → first successful response)

| Server | cold start |
|-------------------------|-----------:|
| Aoxn (native binary) | ~130 ms |
| Node.js 22 | ~340 ms |
| Next.js 15 (`next start`) | ~2.0–3.5 s |

### Build & deploy footprint

| | Aoxn | Node.js | Next.js 15 |
|----------------------|-----------:|------------:|-----------:|
| Build step | `aoxn build` | none | `next build` |
| Cold build | **0.74 s** | — | **66.8 s** |
| Rebuild (content cache) | **0.06 s** | — | — |
| Deploy artifact | 173 KB exe | 1.8 KB script | 56 MB `.next` tree |
| Dependencies to install | 0 | 0 | 305.5 MB (`node_modules`, 8,916 files) |
| Sources (this suite) | 265 lines `.ax` | 49 lines `.js` | ~60 lines `.js` + framework |
| RSS under load | 5 MB | 61 MB | 162–177 MB |

## How the Aoxn server is built

The suite doubles as a demonstration that Aoxn covers systems + web
programming without a runtime:

- **Sockets via C FFI** — `web/sock_win.ax` declares the Winsock2 API
  (`WSAStartup`/`socket`/`bind`/`listen`/`accept`/`recv`/`send`/`setsockopt`)
  with `extern def`, linked with `-l ws2_32`; `web/sock_posix.ax` does the
  same for Linux/macOS libc. Pointers cross the FFI as plain `int`.
- **Zero per-request allocation** — responses render into reusable byte
  buffers (`store_u8`/`memcpy`/hand-rolled itoa). Aoxn strings are immutable
  and concat results are intentionally never freed (documented language
  semantics), so a long-running server renders into byte buffers instead —
  the idiomatic systems answer, and why RSS is flat at 5 MB.
- **Correct HTTP/1.1 framing** — requests accumulate until the `\r\n\r\n`
  terminator and drain one by one, so pipelined requests on a keep-alive
  connection each get a response; responses go out as a single TCP segment
  (header written directly in front of the body) with `TCP_NODELAY` set.

## Caveats

- Client and server share one 4C/8T laptop; absolute numbers are machine
  specific, and the ~7k req/s ceiling caps the Aoxn/Node comparison (the
  latency and in-flight analysis above is what distinguishes them).
- This is a throughput/latency comparison of three routes, not a feature
  comparison: Next.js ships routing, RSC streaming, hydration, ISR, etc.
  The Aoxn server is GET-only HTTP/1.1 by design.
- `next start` on Windows is the official production server as configured;
  standalone/`output: "standalone"` deployments may differ.
- All three bodies were verified equivalent first (Aoxn ↔ Node byte-exact),
  so the throughput numbers compare like with like.
