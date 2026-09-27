# web/ — the Aoxn web benchmark suite

Can Aoxn write web servers? Yes: this directory is a small HTTP/1.1 server
written in Aoxn (raw sockets through `extern def` C FFI + byte-buffer
rendering, zero per-request allocation) benchmarked against the
**pnpm + Node.js + Next.js** stack on identical routes.

All three servers implement the same three routes with the same bodies:

| Route       | Body                                                        |
|-------------|-------------------------------------------------------------|
| `/`         | small SSR HTML page (20-row table + computed sum)           |
| `/api/json` | `{"language":"Aoxn","version":"0.26.3","squares":[...],"sum":2870}` |
| `/text`     | `hello, web\n`                                              |
| `/metrics`  | Prometheus text metrics (requests, bytes, connections, durations) |

The Aoxn and plain-Node bodies are **byte-identical** (verified by SHA-256);
the Next.js page carries the same content with Next's own document markup.

## Layout

| File                       | Purpose                                              |
|----------------------------|------------------------------------------------------|
| `http_buf.ax`              | routing + response rendering into byte buffers       |
| `serve.ax`                 | accept loop, HTTP framing, keep-alive drain          |
| `sock_win.ax`              | Winsock2 wrapper (ws2_32)                            |
| `sock_posix.ax`            | POSIX sockets (Linux / macOS)                        |
| `server_win.ax`            | Windows entry — `aoxn build web\server_win.ax -o web\server.exe -l ws2_32` |
| `server_posix.ax`          | POSIX entry — `aoxn build web/server_posix.ax -o web/server` |
| `node-server.mjs`          | plain `node:http` comparison server                  |
| `next-app/`                | Next.js 15 (app router) comparison app               |
| `loadtest/parity.mjs`      | cross-platform functional test (body parity, keep-alive, /metrics) |
| `loadtest/bench.mjs`       | benchmark orchestrator (oha engine, median of trials)|

## Running the servers

```powershell
# Aoxn (Windows)
cargo run -- build web\server_win.ax -o web\server.exe -l ws2_32
web\server.exe                       # http://127.0.0.1:3000

# Aoxn (Linux / macOS)
cargo run -- build web/server_posix.ax -o web/server
./web/server

# Node.js
node web\node-server.mjs             # PORT env overrides 3000

# Next.js
cd web\next-app
pnpm install
pnpm build
pnpm start
```

## Running the benchmark

The harness uses [oha](https://github.com/hatoo/oha) as the load generator
(far stronger client than autocannon on one core). Download it into
`loadtest/tools/`:

```powershell
# Windows
Invoke-WebRequest https://github.com/hatoo/oha/releases/download/v1.16.0/oha-windows-amd64-pgo.exe -OutFile web\loadtest\tools\oha.exe
```

```powershell
cd web\loadtest
pnpm install        # only for the bench script's runner (no deps today)
pnpm bench          # 3 interleaved trials x 3 servers x 3 routes, median reported
```

Tunables: `BENCH_DURATION` (default `10s`), `BENCH_CONNECTIONS` (32),
`BENCH_TRIALS` (3), `BENCH_PORT` (3000). Results land in
`loadtest/last-results.json`; the interpreted numbers live in
[`docs/web-benchmark.md`](../docs/web-benchmark.md).

Functional tests (body parity vs Node, 404, keep-alive, `/metrics`):

```powershell
node web\loadtest\parity.mjs web\server.exe      # or web/server on POSIX
```

CI runs the same functional tests plus a short reference benchmark on
windows-latest / ubuntu-latest / macos-14
(`.github/workflows/web-bench.yml`) — runner numbers are trend values;
the full protocol is `loadtest/bench.mjs` on dedicated hardware.

Only one server can hold port 3000 at a time — the harness starts/stops each
target itself.
