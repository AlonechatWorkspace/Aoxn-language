# Web 平台 · Web Platform

> **中文**：`web/` 是用 Aoxn 写的 HTTP/1.1 参考服务器及其基准套件——原始 socket 经 `extern def` C FFI、字节缓冲渲染、每请求零分配，在相同路由上与 pnpm + Node.js + Next.js 15 对比；本页说明它的结构、跑法、实测数字与诚实解读，并区分"已实测"与"仅是计划"。
> **English**: `web/` is an HTTP/1.1 reference server written in Aoxn plus its benchmark suite — raw sockets over `extern def` C FFI, byte-buffer rendering, zero per-request allocation — compared against pnpm + Node.js + Next.js 15 on identical routes; this page covers its structure, how to run it, the measured numbers with an honest reading, and what is measured fact versus plan.

## 中文

### 1. 套件定位

`web/` 是**一个用 Aoxn 写的 HTTP/1.1 服务器**：socket 走手写 `extern def` C FFI
（Windows 用 Winsock2，链接 `-l ws2_32`；Linux/macOS 用 libc），响应渲染进**可复用的
字节缓冲**，**每请求零分配**。它与 **pnpm + Node.js + Next.js 15** 在相同路由上对比，
用来回答"网页这条路上 Aoxn 能不能写、写得怎么样"。

它回答的是**基准问题**，不是产品问题：这是一个 GET-only 的 HTTP/1.1 参考服务器，
没有路由框架、静态文件服务、`Range`、压缩、TLS 或异步并发（单线程、一次处理一个连接），
这些都在"未来规划"里（见 §9）。页面上的**吞吐/延迟/启动/足迹数字全部来自
`docs/web-benchmark.md` 与 `web/loadtest/last-results.json`**，本页只做搬运与解读。

### 2. 文件清单

| 文件 | 作用 |
|---|---|
| `web/http_buf.ax` | 路由（`route_of` / `target_is`）+ 响应渲染：`bb_*` 字节缓冲追加、`body_html` / `body_json` / `body_text` / `render_404_body`、`render_metrics`、`render_header` |
| `web/serve.ax` | accept 循环、HTTP 组帧（`find_hdr_end` 扫 `\r\n\r\n`）、keep-alive drain、单段发送、每条请求更新 metrics 块 |
| `web/sock_win.ax` | Winsock2 包装（`WSAStartup` / `socket` / `bind` / `listen` / `accept` / `recv` / `send` / `setsockopt` / `closesocket`），经 `-l ws2_32` 链接 |
| `web/sock_posix.ax` | POSIX socket 包装（Linux / macOS），同接口 |
| `web/server_win.ax` | Windows 入口：`def main() -> int: return serve(3000)` |
| `web/server_posix.ax` | POSIX 入口，同上 |
| `web/node-server.mjs` | 纯 `node:http` 对照服务器（渲染逐字节相同的响应体） |
| `web/next-app/` | Next.js 15（app router）对照应用：`app/layout.js`、`app/page.js`、`app/api/`、`app/text/`；`pnpm build` / `pnpm start` |
| `web/loadtest/parity.mjs` | 跨平台功能测试（响应体一致性、404、keep-alive、`/metrics`） |
| `web/loadtest/bench.mjs` | 压测编排：oha 引擎、3 轮交错、取中位数、采样 RSS |
| `web/loadtest/package.json` | `pnpm bench` → `node bench.mjs` |
| `web/loadtest/last-results.json` | 最近一次原始结果（结构：`duration` / `connections` / `trials` / `readyMs` / `runs[]` / `aggregate[]`） |
| `web/loadtest/tools/` | oha 二进制下载目标（gitignored） |
| `.github/workflows/web-bench.yml` | 三平台矩阵：构建服务器 + 功能一致性 + 短参考基准 |

### 3. 路由与"逐字节一致"这个前提

| 路由 | 响应体 | 路由码 | 渲染函数 |
|---|---|---|---|
| `/` | SSR HTML 页面：20 行 `<tr>` 表格 + 计算出的平方和 | 0 | `body_html` |
| `/api/json` | `{"language":"Aoxn","version":"0.26.3","squares":[...],"sum":2870}` | 1 | `body_json` |
| `/text` | `hello, web\n` | 2 | `body_text` |
| `/metrics` | Prometheus 文本指标（见 §7） | 4 | `render_metrics` |
| 其它任意路径 | `not found\n` + `HTTP/1.1 404 Not Found` | 3 | `render_404_body` |

**前提**：Aoxn 与 plain Node 的响应体在三条基准路由上 **SHA-256 逐字节一致**
（`docs/web-benchmark.md`；`parity.mjs` 每次运行都重新校验）。Next.js 页面携带**相同内容**
（标题、表格、和），但带 Next 自己的文档标记与 hydration payload——所以吞吐对比是
"同内容、不同框架开销"，而不是"同一份字节"。

### 4. 构建与运行

来自 `web/README.md`（Windows 与 POSIX 两套）：

```powershell
# Aoxn（Windows）
cargo run -- build web\server_win.ax -o web\server.exe -l ws2_32
web\server.exe                       # http://127.0.0.1:3000

# Node.js 对照
node web\node-server.mjs             # PORT 环境变量可覆盖 3000

# Next.js 对照
cd web\next-app
pnpm install
pnpm build
pnpm start
```

```bash
# Aoxn（Linux / macOS）
cargo run -- build web/server_posix.ax -o web/server
./web/server                         # http://127.0.0.1:3000
```

功能测试（与 Node 的响应体一致性、404、keep-alive、`/metrics`）：

```powershell
node web\loadtest\parity.mjs web\server.exe      # POSIX 上是 web/server
```

### 5. 基准怎么跑

压测引擎是 [oha](https://github.com/hatoo/oha)（单核上比 autocannon 强得多；本套件
不用 autocannon，因为单核客户端更容易成为瓶颈、从而掩盖服务端差异——这条判断来自
维护者的本地测量，未写进仓库文档）。先把 oha 下载到 `loadtest/tools/`：

```powershell
# Windows
Invoke-WebRequest https://github.com/hatoo/oha/releases/download/v1.16.0/oha-windows-amd64-pgo.exe -OutFile web\loadtest\tools\oha.exe
```

```powershell
cd web\loadtest
pnpm install        # 只为 bench 脚本的 runner（当前无依赖）
pnpm bench          # 3 轮交错 × 3 个服务器 × 3 条路由，报告取中位数
```

- **调优项**：`BENCH_DURATION`（默认 `10s`）、`BENCH_CONNECTIONS`（32）、
  `BENCH_TRIALS`（3）、`BENCH_PORT`（3000）。
- **结果落点**：`web/loadtest/last-results.json`；解读后的数字在
  `docs/web-benchmark.md`。
- **编排细节**（`bench.mjs`）：每个目标依次在 `127.0.0.1:3000` 启动，等端口就绪
  （记入 `readyMs`）、3s 预热，再逐路由压测；整个矩阵按 trial 交错跑 `BENCH_TRIALS`
  轮，取**中位数**；每 2s 采样一次 RSS；`GITHUB_STEP_SUMMARY` 存在时追加一张 Markdown
  表；开始前有端口预检（上一个测试残留的监听会让每个目标"提前退出"）。
  **3000 端口同时只能有一个服务器**——套件自己负责启停。
- POSIX 下从 CI 的做法照抄：`oha-linux-amd64-pgo` / `oha-macos-arm64`（Intel Mac 用
  `oha-macos-amd64`）下载到 `loadtest/tools/oha` 并 `chmod +x`。

### 6. 结果解读

测量条件（`docs/web-benchmark.md` §Environment & method）——**引用任何数字都要带上它们**：
Windows on **Intel Core i5-1135G7 (4C/8T)**、LLVM 23.1.0、MSVC Build Tools；
oha 1.16.0（PGO 构建）、**32 个 keep-alive 连接**、每轮 **10s**；
**压测客户端与服务器同机**（127.0.0.1）；每（服务器, 路由）**3 轮交错取中位数**；
每个服务器测量前预热 3s。

#### 6.1 吞吐与延迟（3 × 10s 的中位数，32 连接）

| Server | Route | req/s | p50 | p95 | p99 | errors |
|---|---|---|---|---|---|---|
| **Aoxn (O3 native)** | `/text` | **7,611** | 0.094 ms | 0.256 ms | 0.787 ms | 0 |
| **Aoxn (O3 native)** | `/api/json` | **7,623** | 0.100 ms | 0.259 ms | 0.642 ms | 0 |
| **Aoxn (O3 native)** | `/` | **6,942** | 0.097 ms | 0.298 ms | 0.826 ms | 0 |
| Node.js 22 (node:http) | `/text` | 7,010 | 4.071 ms | 8.422 ms | 12.189 ms | 0 |
| Node.js 22 (node:http) | `/api/json` | 6,330 | 4.619 ms | 9.304 ms | 12.980 ms | 0 |
| Node.js 22 (node:http) | `/` | 5,892 | 4.769 ms | 10.590 ms | 15.104 ms | 0 |
| Next.js 15 (`next start`) | `/text` | 186 | 143.8 ms | 321.1 ms | 801.5 ms | 0 |
| Next.js 15 (`next start`) | `/api/json` | 142 | 166.4 ms | 612.1 ms | 1,487.0 ms | 0 |
| Next.js 15 (`next start`) | `/` | 263 | 116.0 ms | 162.9 ms | 197.5 ms | 0 |

文档自己的总结：**Aoxn vs Next.js**：吞吐 26–54×、p50 延迟 1/1,200–1/1,700，进程
小 33×（5 MB vs 162–177 MB RSS）；**Aoxn vs Node.js**：吞吐相当（见下），p50 延迟低
约 **40–50×**。

#### 6.2 诚实解读：~7.6k req/s 是压测端上限，不是服务端上限

按 Little's law（在途请求数 = req/s × 平均延迟），从同一批中位数算出：

| Server | 在途请求（共 32 连接） | 解读 |
|---|---|---|
| Aoxn | **≈ 0.7** | 服务器 >95% 时间空闲；真实容量远高于测到的 7.6k req/s |
| Node.js | **≈ 28** | 连接在排队；服务器已到极限 |
| Next.js | **≈ 28** | 同上，且每请求有 ~120 ms 的管线成本 |

双客户端扩展性检查（2 × 32 连接）印证了这一点：**Node 的 p50 翻倍（4 → 9.7 ms），
Aoxn 不变（0.10 ms）**，而合计吞吐对两者都停在这台机器 ~6–7k req/s 的请求生成上限上。
空载时 Next.js 单独每请求就要 **16–23 ms**（curl 计时），这就是它只能跑到几百 req/s 的原因。

因此本套件的正确用法是：**不要用原始 req/s 区分 Aoxn 与 Node，用延迟与在途请求数**；
吞吐列只是证明 Aoxn 没有落后。

#### 6.3 启动时间（spawn → 首个成功响应）

| Server | 冷启动 |
|---|---|
| Aoxn（原生二进制） | ~130 ms |
| Node.js 22 | ~340 ms |
| Next.js 15 (`next start`) | ~2.0–3.5 s |

`last-results.json` 里那次运行的 `readyMs` 分别是 **149 ms / 311 ms / 2010 ms**，
与上面的表在同一量级（文档表是取整区间，JSON 是单次实测值）。

#### 6.4 构建与部署足迹

| | Aoxn | Node.js | Next.js 15 |
|---|---|---|---|
| 构建步骤 | `aoxn build` | 无 | `next build` |
| 冷构建 | **0.74 s** | — | **66.8 s** |
| 重建（内容缓存） | **0.06 s** | — | — |
| 部署产物 | 173 KB exe | 1.8 KB 脚本 | 56 MB `.next` 目录树 |
| 需要安装的依赖 | 0 | 0 | 305.5 MB（`node_modules`，8,916 个文件） |
| 源码（本套件） | 265 行 `.ax` | 49 行 `.js` | ~60 行 `.js` + 框架 |
| 压测下 RSS | 5 MB | 61 MB | 162–177 MB |

行数口径提醒："265 行 `.ax`" 是 `docs/web-benchmark.md` 表里的数字；按 v0.26.3 工作树
直接统计，`web/*.ax` 六个文件（含注释）共 **473 行**。两者口径不同，
引用时请注明出自哪一处（本页数字一律以文档表为准）。

#### 6.5 限定条件（保留原文）

- 客户端与服务器共享同一台 4C/8T 笔记本；绝对值是机器相关的，且 **~7k req/s 的上限
  卡死了 Aoxn/Node 的吞吐对比**（真正区分它们的是上文的延迟与在途请求分析）。
- 这是**三条路由的吞吐/延迟对比，不是功能对比**：Next.js 自带路由、RSC 流式渲染、
  hydration、ISR 等；Aoxn 服务器按设计是 **GET-only HTTP/1.1**。
- Windows 上的 `next start` 就是官方生产服务器形态；`output: "standalone"` 部署可能不同。
- 三个响应体先验证过等价（Aoxn ↔ Node 逐字节一致），因此吞吐数字是可比口径。

### 7. 服务器编写上的 Aoxn 侧要点

这些都是 `web/*.ax` 源码里真实采用的写法，也是"为什么这么写"的答案。

**为什么用 `bb_*` 字节缓冲而不是字符串拼接。** Aoxn 字符串不可变，且**拼接结果按语言语义
永不释放**（没有 GC，见 [语言参考](Language-Reference.md)）。长跑服务器如果每请求都
`s = s + piece`，RSS 会无界增长。所以响应渲染进调用方提供的字节缓冲：`bb_byte` /
`bb_str` / `bb_int` / `bb_crlf` 都**返回新的长度**（`n = bb_str(p, n, "...")`），
缓冲只分配一次（`rbuf` 8192、`hbuf` 1024、`bbuf` 16384），JSON 的 `sum` 与表格的平方和
直接在缓冲里逐位渲染（`bb_int` 从最大十次幂往下逐位输出，不需要 scratch）。结果是
压测下 RSS 恒定在 5 MB。这是**惯用的系统级写法，不是 bug**。

**没有 `\r` 转义。** 语言的字符串转义只有 `\n`、`\t`、`\\`、`\"`，所以 HTTP 的 CRLF
由 `bb_crlf` 写**原始字节 13/10**（`store_u8(p, n, 13)` 然后 `10`），而不是 `"\r\n"`。

**原始内存内建的参数个数不一样。** `load_u8(base, off)` 是**两个参数**；
`load_i64(addr)` 只有**一个参数**（地址），偏移要自己加在地址上——例如
`load_i64(m + 48)`；写侧同理 `store_i64(m + 96, d)`。`str_get(pat, k)` 用来逐字节
比较字符串（`target_is` 就是靠它 + `load_u8` 做精确匹配的）。

**C `int` 返回值会零扩展成 i64。** 被调方写 EAX，所以 `-1` 到达 Aoxn 时显示为
**4294967295**。`sock_win.ax` / `sock_posix.ax` 里的 `i32()` 辅助函数把它映射回 `-1`
（真正的 64 位 `-1` 原样通过）；SOCKET/指针返回值是完整 64 位。Windows 上连
`recv`/`send` 都是 C `int`（要过 `i32()`），POSIX 上它们返回 `ssize_t`，直接透传。
这个坑值得单独记住：**判断 socket 调用失败前先过 `i32()`**。

**响应作为单段发送 + `TCP_NODELAY`。** `send_response` 先调 `render_body(code, bbuf, 512, m)`
得到**新的绝对偏移**（`bend`），body 长度是 `bend - 512`；再把 header 渲染进 `hbuf`，
用一次 `memcpy` 拷到 `bbuf + 512 - hn`，让 header 紧贴 body 前面，最后**一次
`net_send_all`** 发出去。每个 accept 到的连接都设 `TCP_NODELAY`
（`setsockopt(s, 6, 1, optval, 4)`，`optval` 是指向 int 1 的 4 字节缓冲）。
把 header 和 body 拆成两次 send 会被 **Nagle 算法拖到毫秒级/请求**——这正是"单段发送"
存在的理由。

**HTTP 组帧。** `find_hdr_end` 扫 `\r\n\r\n`（13/10/13/10）得到 header 结束位置，
`req_len = e + 4`；一次 `recv` 里的多个请求（keep-alive 流水线）逐个 drain，
剩余字节用一个**逐字节循环**滑到缓冲区开头（源与目标可能重叠，`memcpy` 重叠是 UB），
不完整的请求留在缓冲里等下一次 `recv`。缓冲区写满（`used == 8192`）即关闭连接。

**`/metrics` 的内容。** Prometheus 文本格式，按 RED 组织（`http_buf.ax` 的
`render_metrics`；metrics 块槽位布局写在它上方的注释里）：

```text
# TYPE aoxn_http_requests_total counter
aoxn_http_requests_total{route="/"} <n>
aoxn_http_requests_total{route="/api/json"} <n>
aoxn_http_requests_total{route="/text"} <n>
aoxn_http_requests_total{route="/metrics"} <n>
aoxn_http_requests_total{route="other"} <n>
# TYPE aoxn_http_not_found_total counter
# TYPE aoxn_http_response_bytes_total counter
# TYPE aoxn_http_connections_total counter
# TYPE aoxn_http_connections_active gauge
# TYPE aoxn_http_request_duration_nanoseconds_sum counter
# TYPE aoxn_http_request_duration_nanoseconds_max gauge
# TYPE aoxn_http_uptime_seconds gauge
```

Content-Type 是 `text/plain; version=0.0.4; charset=utf-8`。metrics 块是一个 112 字节的
`malloc`，槽位为：0 启动秒、8 请求数、16 出字节、24 未命中（404）、32 累计连接、
40 活跃连接、48+8×路由码 每路由计数、88 耗时总和(ns)、96 耗时最大(ns)、
104 时钟 scratch 指针。每请求用 `net_now_ns(m)` 在路由+渲染+发送前后各取一次时间来算耗时。
`net_now_ns` 在平台层实现：Windows 用 `QueryPerformanceCounter`/`QueryPerformanceFrequency`，
POSIX 用 `clock_gettime`。**坑**：`timespec_get` 在 Windows 上（clang/MSVC 库）链接不过，
别重试。

**平台差异（POSIX 包装里真实存在）。** `bind` 之前必须 `setsockopt(SO_REUSEADDR)`，否则重启
或上一个测试服务器留下的 TIME_WAIT 会让 Linux/macOS 上的 bind 报 `EADDRINUSE`。**两个常量
都分平台**：level `SOL_SOCKET` 在 Linux 上是 `1`、在 macOS/BSD 上是 `65535`（`0xFFFF`）；
optname `SO_REUSEADDR` 在 Linux 上是 `2`、在 macOS/BSD 上是 `4`。`web/sock_posix.ax` 按
`target_os()` 分两支（`setsockopt(s, 1, 2, opt, 4)` / `setsockopt(s, 65535, 4, opt, 4)`）——
把 Linux 的 level `1` 用到 macOS 上会**静默失败**，bug 照旧复现。Windows 的 Winsock 语义不同，
**不要**在 Windows 上加它（会允许端口劫持）。`sockaddr_in` 的填充也分平台：macOS 是
`store_u8(addr, 0, 16)` + `store_u8(addr, 1, 2)`，Linux 是小端 `store_u8(addr, 0, 2)`。
这些分支都用编译期内建 `target_os()` 选择。

**线程模型。** 单线程 accept 循环，一次完整处理一个连接（收到 EOF/错误后 `closesocket`
并递减活跃连接数）。所以"每请求零分配 + 单段发送"不只是性能选择，也是这个模型的自然形态。

### 8. 功能测试与 CI

`web/loadtest/parity.mjs`（跨平台功能测试，同时启动 Aoxn 与 Node 参考服务器）验证：

1. **响应体一致性**：`/text`、`/api/json`、`/` 三个 body 与 Node 参考实现**逐字节相同**
   （`Buffer.equals`，失败时打印两侧字节数）；
2. **404 处理**：未知路由返回状态 404 且 body 恰好是 `not found\n`；
3. **keep-alive**：连续 3 次请求都能拿到 `hello, web\n`；
4. **`/metrics`**：存在 `# TYPE aoxn_http_requests_total counter`、
   `aoxn_http_uptime_seconds`、`aoxn_http_connections_total`，且计数确实反映了本次测试
   （`route="/text"` ≥ 1、404 数 ≥ 1）。

全部通过打印 `parity: ALL PASS` 并以 0 退出，否则按失败数退出非零。

`.github/workflows/web-bench.yml` 在 **windows-latest / ubuntu-latest / macos-14** 三平台
的矩阵上跑：

- 构建编译器（`cargo build --release`）→ 构建服务器
  （Windows：`web/server_win.ax` → `web/server.exe`，`-l ws2_32`；
  Linux：`web/server_posix.ax` → `web/server`，LLVM 用 `llvm-18-dev`；
  macOS：`/opt/homebrew/opt/llvm@18`）；
- **功能一致性**：`node web/loadtest/parity.mjs <产物>`；
- 构建 Next.js 对照应用（`pnpm install --frozen-lockfile` + `pnpm build`）；
- 安装 oha 1.16.0，跑**短参考基准**（`BENCH_TRIALS=1`、`BENCH_DURATION=5s`），
  结果进 job summary；最后把 `web/loadtest/last-results.json` 作为
  `web-bench-<os>` 产物上传。

触发条件：`web/**` 或该 workflow 文件变更时的 push / PR，以及手动 `workflow_dispatch`。
**CI runner 共享 CPU，数字只能当趋势/回归看，不是绝对值**；正式对比仍以专用机器上全量
`bench.mjs`（3 轮交错取中位数）为准。

### 9. 未来规划（这些是计划，不是现状）

以下全部来自 `docs/web-platform-plan.md`（v0 设计讨论），**除标"已完成"的 W0 外都还没落地**：
**`web/` 现在只是一个基准套件**，不是一个 Web 框架或产品服务器。

**已决策的方向**

- **TS 前端（方案 A）**：新增 TypeScript 语法的 lexer/parser，产出与现有 Aoxn 管线相同的
  AST/IR，直通 LLVM——目标是现有 `*.ts`/`*.tsx` 项目不改习惯即可编译运行，Aoxn 的
  Python 风格语法退居实现层。关键工程量有两块：TS 的类型系统（结构化类型、联合/交叉/
  字面量类型、控制流收窄、`any`、可选、元组）需要在 typecheck 层重建；JS 的运行时语义
  （对象引用语义 + 原型链 + 闭包 + GC）是"完整兼容"里最大的一块，必须分期。
- **模块系统**：移除现有 `import "相对路径"`，改 TS 式 `import ... from "pkg"` +
  node_modules 式自底向上查找与 `package.json` 的 `exports`/`main`/`types`；影响 Rust 侧
  loader、`selfhost/load.ax`、`stdlib/`（一次切换、不留双轨——节奏是待决项【C】）。
- **独立包管理（方案 A2）**：自有 manifest（暂称 `aoxn.json`）+ 自有 lockfile + 自建
  registry（协议对齐 npm Registry API 的成熟子集、支持私有 scope 与鉴权）+ 不读
  node_modules 的解析器；配一次性 npm 导入工具或注册表代理层作为生态桥接
  （工程量：包管理器约 4–6 周，registry 与桥接另计）。
- **样式（方案 B1）**：`.css` 为一等资源 + 构建管线（打包/压缩/指纹/缓存协商）；
  CSS Modules 局部类名哈希 + 类型化 class map 导入；兼容 Tailwind 工具链（JIT 产物接入
  同一资源管线）；二期的 CSS-in-TS 类型化封装作为可选糖；图形先用 SVG + CSS。

**里程碑（计划表）**

| 阶段 | 内容 | 状态 |
|---|---|---|
| **W0** | `/metrics` 可观测性 + `parity.mjs` + 三平台 CI（`web-bench.yml`）+ 设计文档 | **已完成** |
| **W1** | TS-M1 前端（语法子集 → 现有管线）+ 新模块系统（import/export）+ 移除旧 `import`；stdlib/selfhost 同批迁移 | **进行中**：S0 词法器 + S1 解析器已完成（`src/ts/`，`.ts` 端到端编译运行验证，见 [前端](Frontend-Lexer-and-Parser.md) §11 与 `docs/ts-m1-spec.md`）；模块系统与旧 `import` 移除在 S3 |
| **W2** | 独立包管理（manifest/lockfile/registry 客户端 + npm 桥接）；CSS 资源管线 + CSS Modules + Tailwind；静态文件服务 + `Range` + 缓存头 | 按 A2/B1 决策展开 |
| **W3** | 基准 v2 全量执行（复杂场景 + 浏览器渲染指标 + 编译时长）、Linux/macOS 专用机数据 | 依赖 W2 |
| **W4** | TS-M2 运行时语义（对象/闭包/GC）→ 团队真实项目迁移验收 | 依赖样本项目 |

TS 的里程碑切分是：**TS-M1** 语法 + 静态子集（标注类型、`interface`、`enum`、泛型、
函数/箭头函数、`let`/`const`、模板字符串、`class` 字段与方法、import/export）→
**TS-M2** JS 运行时语义（对象引用语义、闭包、常用数组/Map 库、GC 或以区域分配起步）→
**TS-M3** 全量（async/Promise、装饰器、namespace、`lib.dom`/`node.d.ts` 常用面）。

**基准 v2 的指标设计（计划）**：请求耗时（TTFB、完整响应 p50/p95/p99、冷启动首请求、
keep-alive 与新建连接分开报）、**页面真实渲染耗时**（Playwright + CDP tracing 的
FCP/LCP/TTI，与服务端 SSR 耗时分开报）、编译时长（小/中/大三档 × 冷/热缓存/增量；
`aoxn build` vs `next build` vs `tsc --noEmit`）；复杂场景 S0 空页（现有基线）、
S1 图文页（20 张图、50KB CSS、字体）、S2 重资源页（`<video>` 流式 + `Range`）、
S3 静态资源吞吐（图片/CSS/字体、缓存头、gzip/br、ETag）、S4 混合压测（70/20/10，
持续 5 分钟看稳定性/内存）。**配套能力是当前 Aoxn 服务器还需要补的**：静态文件服务、
`Range`、缓存协商（ETag/Cache-Control）、可选 gzip。可观测性四件套里 `/metrics`
（RED）已落地，`/healthz` 与结构化 JSON 访问日志计划随 TS 栈一起，请求级 traces
（可选 OTLP 导出）属二期。

**仍未拍板的事项**：【C】旧 `import` 的移除节奏；npm 桥接方案（一次性导入工具 vs
注册表代理层，影响 Tailwind 等 npm 工具的接入方式）；TS-M1 的 2–3 个团队真实项目验收
样本；页面渲染指标是否采用 Playwright + Chrome；Linux/macOS 正式基准用 CI 参考值还是
专用机。

### 10. 与性能页的分工

**本页讲"用 Aoxn 写的服务器有多快"**：套件结构、路由、跑法、功能测试/CI、
以及吞吐/延迟/启动/RSS 的实测表格与 Little's law 解读。
**[性能与基准](Performance-and-Benchmarks.md) 讲"编译器有多快、生成代码有多快"**：
编译耗时结构、优化级别取舍、构建缓存、链接/启动地板与测量纪律。
两页数字出处不同（本页主要来自 `docs/web-benchmark.md` 与
`web/loadtest/last-results.json`），不要交叉混用。

## English

### 1. What this suite is

`web/` is **an HTTP/1.1 server written in Aoxn**: sockets go through hand-written
`extern def` C FFI (Winsock2 on Windows, linked with `-l ws2_32`; libc on Linux/macOS),
responses render into **reusable byte buffers**, and serving allocates **nothing per
request**. It is benchmarked against **pnpm + Node.js + Next.js 15** on identical routes,
to answer "can Aoxn do web work, and how does it compare?"

It answers a **benchmark** question, not a product question: this is a GET-only HTTP/1.1
reference server with no routing framework, static file service, `Range` support,
compression, TLS or async concurrency (single-threaded, one connection at a time). Those
belong to the plan in §9. Every **throughput/latency/startup/footprint number on this page
comes from `docs/web-benchmark.md` and `web/loadtest/last-results.json`**; this page only
carries them over and reads them.

### 2. File inventory

| File | Purpose |
|---|---|
| `web/http_buf.ax` | routing (`route_of` / `target_is`) plus response rendering: `bb_*` byte-buffer appends, `body_html` / `body_json` / `body_text` / `render_404_body`, `render_metrics`, `render_header` |
| `web/serve.ax` | accept loop, HTTP framing (`find_hdr_end` scans for `\r\n\r\n`), keep-alive drain, single-segment send, per-request metrics updates |
| `web/sock_win.ax` | Winsock2 wrapper (`WSAStartup` / `socket` / `bind` / `listen` / `accept` / `recv` / `send` / `setsockopt` / `closesocket`), linked with `-l ws2_32` |
| `web/sock_posix.ax` | POSIX socket wrapper (Linux / macOS), same interface |
| `web/server_win.ax` | Windows entry: `def main() -> int: return serve(3000)` |
| `web/server_posix.ax` | POSIX entry, same |
| `web/node-server.mjs` | plain `node:http` comparison server (renders byte-identical bodies) |
| `web/next-app/` | Next.js 15 (app router) comparison app: `app/layout.js`, `app/page.js`, `app/api/`, `app/text/`; `pnpm build` / `pnpm start` |
| `web/loadtest/parity.mjs` | cross-platform functional test (body parity, 404, keep-alive, `/metrics`) |
| `web/loadtest/bench.mjs` | benchmark orchestrator: oha engine, 3 interleaved trials, median, RSS sampling |
| `web/loadtest/package.json` | `pnpm bench` → `node bench.mjs` |
| `web/loadtest/last-results.json` | the latest raw results (shape: `duration` / `connections` / `trials` / `readyMs` / `runs[]` / `aggregate[]`) |
| `web/loadtest/tools/` | download target for the oha binary (gitignored) |
| `.github/workflows/web-bench.yml` | three-platform matrix: build the server, run parity, run a short reference benchmark |

### 3. Routes and the byte-identical premise

| Route | Body | Route code | Renderer |
|---|---|---|---|
| `/` | SSR HTML page: a 20-row `<tr>` table plus the computed sum of squares | 0 | `body_html` |
| `/api/json` | `{"language":"Aoxn","version":"0.26.3","squares":[...],"sum":2870}` | 1 | `body_json` |
| `/text` | `hello, web\n` | 2 | `body_text` |
| `/metrics` | Prometheus text metrics (see §7) | 4 | `render_metrics` |
| anything else | `not found\n` with `HTTP/1.1 404 Not Found` | 3 | `render_404_body` |

**Premise**: the Aoxn and plain-Node response bodies are **byte-identical (SHA-256
verified)** on all three benchmark routes (`docs/web-benchmark.md`; `parity.mjs` re-checks
this on every run). The Next.js page carries the **same content** (title, table, sum) with
Next's own document markup and hydration payload — so the throughput comparison is
"same content, different framework overhead", not "same bytes".

### 4. Building and running

From `web/README.md` (Windows and POSIX):

```powershell
# Aoxn (Windows)
cargo run -- build web\server_win.ax -o web\server.exe -l ws2_32
web\server.exe                       # http://127.0.0.1:3000

# Node.js reference
node web\node-server.mjs             # the PORT env var overrides 3000

# Next.js reference
cd web\next-app
pnpm install
pnpm build
pnpm start
```

```bash
# Aoxn (Linux / macOS)
cargo run -- build web/server_posix.ax -o web/server
./web/server                         # http://127.0.0.1:3000
```

Functional tests (body parity vs Node, 404, keep-alive, `/metrics`):

```powershell
node web\loadtest\parity.mjs web\server.exe      # web/server on POSIX
```

### 5. Running the benchmark

The load generator is [oha](https://github.com/hatoo/oha) (far stronger than autocannon
on one core; this suite does not use autocannon because a single-core client is the likelier
bottleneck and would mask server differences — that judgement comes from local measurement
and is not written up in the repository docs). Download it into `loadtest/tools/` first:

```powershell
# Windows
Invoke-WebRequest https://github.com/hatoo/oha/releases/download/v1.16.0/oha-windows-amd64-pgo.exe -OutFile web\loadtest\tools\oha.exe
```

```powershell
cd web\loadtest
pnpm install        # only for the bench script's runner (no deps today)
pnpm bench          # 3 interleaved trials x 3 servers x 3 routes, median reported
```

- **Tunables**: `BENCH_DURATION` (default `10s`), `BENCH_CONNECTIONS` (32),
  `BENCH_TRIALS` (3), `BENCH_PORT` (3000).
- **Where results land**: `web/loadtest/last-results.json`; the interpreted numbers are in
  `docs/web-benchmark.md`.
- **Orchestration details** (`bench.mjs`): each target is started in turn on
  `127.0.0.1:3000`, waited on until ready (recorded in `readyMs`), warmed up for 3s, then
  every route is load-tested; the whole matrix runs `BENCH_TRIALS` times **interleaved**
  and the **median** run is reported; RSS is sampled every 2s; a Markdown table is
  appended to `GITHUB_STEP_SUMMARY` when set; a port pre-flight runs first (a leaked
  listener from a previous test step would make every target "exit early").
  **Only one server can hold port 3000 at a time** — the harness starts and stops each
  target itself.
- On POSIX, mirror the CI: fetch `oha-linux-amd64-pgo` / `oha-macos-arm64`
  (`oha-macos-amd64` on Intel Macs) into `loadtest/tools/oha` and `chmod +x` it.

### 6. Reading the results

Measurement conditions (`docs/web-benchmark.md` §Environment & method) — **quote them
with any number you reuse**: Windows on an **Intel Core i5-1135G7 (4C/8T)**, LLVM 23.1.0,
MSVC Build Tools; oha 1.16.0 (PGO build), **32 keep-alive connections**, **10s** per run;
**client and servers on the same machine** (127.0.0.1); **3 interleaved trials per
(server, route), median reported**; each server warmed up for 3s before measurement.

#### 6.1 Throughput and latency (median of 3 × 10s, 32 connections)

| Server | Route | req/s | p50 | p95 | p99 | errors |
|---|---|---|---|---|---|---|
| **Aoxn (O3 native)** | `/text` | **7,611** | 0.094 ms | 0.256 ms | 0.787 ms | 0 |
| **Aoxn (O3 native)** | `/api/json` | **7,623** | 0.100 ms | 0.259 ms | 0.642 ms | 0 |
| **Aoxn (O3 native)** | `/` | **6,942** | 0.097 ms | 0.298 ms | 0.826 ms | 0 |
| Node.js 22 (node:http) | `/text` | 7,010 | 4.071 ms | 8.422 ms | 12.189 ms | 0 |
| Node.js 22 (node:http) | `/api/json` | 6,330 | 4.619 ms | 9.304 ms | 12.980 ms | 0 |
| Node.js 22 (node:http) | `/` | 5,892 | 4.769 ms | 10.590 ms | 15.104 ms | 0 |
| Next.js 15 (`next start`) | `/text` | 186 | 143.8 ms | 321.1 ms | 801.5 ms | 0 |
| Next.js 15 (`next start`) | `/api/json` | 142 | 166.4 ms | 612.1 ms | 1,487.0 ms | 0 |
| Next.js 15 (`next start`) | `/` | 263 | 116.0 ms | 162.9 ms | 197.5 ms | 0 |

The document's own summary: **Aoxn vs Next.js** — 26–54× the throughput at
1/1,200–1/1,700 the p50 latency, from a process 33× smaller (5 MB vs 162–177 MB RSS);
**Aoxn vs Node.js** — comparable throughput (see below) at roughly **40–50× lower** p50
latency.

#### 6.2 The honest reading: ~7.6k req/s is a load-generator ceiling, not the server's

Applying Little's law (in-flight requests = req/s × mean latency) to the same medians:

| Server | In-flight requests (of 32 connections) | Interpretation |
|---|---|---|
| Aoxn | **≈ 0.7** | the server is idle >95% of the time; its real capacity is far above the measured 7.6k req/s |
| Node.js | **≈ 28** | connections are queueing; the server is at its limit |
| Next.js | **≈ 28** | same, with ~120 ms of per-request pipeline cost |

A two-client scaling check (2 × 32 connections) confirms it: **Node's p50 doubled
(4 → 9.7 ms) while Aoxn's was unchanged (0.10 ms)**, and combined throughput stayed at
this machine's ~6–7k req/s request-generation ceiling for both. Unloaded, Next.js alone
needs **16–23 ms per request** (curl timing), which is why it saturates at a few hundred
req/s.

So the correct way to use this suite is: **do not tell Aoxn and Node apart by raw req/s;
use latency and in-flight requests**. The throughput column only shows Aoxn is not behind.

#### 6.3 Startup (spawn → first successful response)

| Server | Cold start |
|---|---|
| Aoxn (native binary) | ~130 ms |
| Node.js 22 | ~340 ms |
| Next.js 15 (`next start`) | ~2.0–3.5 s |

The `readyMs` values in that particular `last-results.json` run were **149 ms / 311 ms /
2010 ms**, the same order as the table above (the document's table is a rounded range;
the JSON records single measured values).

#### 6.4 Build and deploy footprint

| | Aoxn | Node.js | Next.js 15 |
|---|---|---|---|
| Build step | `aoxn build` | none | `next build` |
| Cold build | **0.74 s** | — | **66.8 s** |
| Rebuild (content cache) | **0.06 s** | — | — |
| Deploy artifact | 173 KB exe | 1.8 KB script | 56 MB `.next` tree |
| Dependencies to install | 0 | 0 | 305.5 MB (`node_modules`, 8,916 files) |
| Sources (this suite) | 265 lines `.ax` | 49 lines `.js` | ~60 lines `.js` + framework |
| RSS under load | 5 MB | 61 MB | 162–177 MB |

A note on the line-count basis: "265 lines `.ax`" is the figure in the
`docs/web-benchmark.md` table; counting the v0.26.3 working tree directly, the six
`web/*.ax` files total **473 lines** including comments. The two figures use different
bases — say which one you are quoting (this page always quotes the documented table).

#### 6.5 Caveats (kept from the source)

- Client and server share one 4C/8T laptop; absolute numbers are machine-specific, and the
  **~7k req/s ceiling caps the Aoxn/Node throughput comparison** (the latency and
  in-flight analysis above is what distinguishes them).
- This is a **throughput/latency comparison of three routes, not a feature comparison**:
  Next.js ships routing, RSC streaming, hydration, ISR and more; the Aoxn server is
  **GET-only HTTP/1.1 by design**.
- `next start` on Windows is the official production server as configured;
  `output: "standalone"` deployments may differ.
- All three bodies were verified equivalent first (Aoxn ↔ Node byte-exact), so the
  throughput numbers compare like with like.

### 7. Aoxn-side notes on writing the server

These are the idioms the `web/*.ax` sources actually use, and the reasons behind them.

**Why `bb_*` byte buffers instead of string concatenation.** Aoxn strings are immutable
and **concat results are never freed by design** (there is no GC — see
[Language Reference](Language-Reference.md)). A long-running server that did
`s = s + piece` per request would grow RSS without bound. Responses therefore render into
caller-provided byte buffers: `bb_byte` / `bb_str` / `bb_int` / `bb_crlf` all **return the
new length** (`n = bb_str(p, n, "...")`), the buffers are allocated once (`rbuf` 8192,
`hbuf` 1024, `bbuf` 16384), and numbers (the JSON `sum`, the table's squares) are rendered
digit by digit straight into the buffer (`bb_int` walks down from the largest power of ten,
so no scratch space is needed). The payoff is an RSS that stays flat at 5 MB under load.
This is **idiomatic systems code, not a bug**.

**There is no `\r` escape.** The language's string escapes are only `\n`, `\t`, `\\`, `\"`,
so HTTP CRLF is written as **raw bytes 13/10** by `bb_crlf`
(`store_u8(p, n, 13)` then `10`) rather than `"\r\n"`.

**The raw-memory builtins do not take the same number of arguments.** `load_u8(base, off)`
takes **two**; `load_i64(addr)` takes **one** (an address), so offsets go on the address —
e.g. `load_i64(m + 48)`, and on the write side `store_i64(m + 96, d)`. `str_get(pat, k)`
compares string bytes one at a time (that plus `load_u8` is how `target_is` matches a
target exactly).

**C `int` returns arrive zero-extended into i64.** The callee writes EAX, so `-1` shows up
in Aoxn as **4294967295**. The `i32()` helper in `sock_win.ax` / `sock_posix.ax` maps it
back (a true 64-bit `-1` passes through unchanged); SOCKET/pointer returns are full 64-bit.
On Windows even `recv`/`send` are C `int` (so they go through `i32()`); on POSIX they
return `ssize_t` and pass straight through. The lesson worth remembering: **run socket
results through `i32()` before testing for failure**.

**Responses go out as a single segment, with `TCP_NODELAY`.** `send_response` first calls
`render_body(code, bbuf, 512, m)`, which returns the **new absolute offset** (`bend`), so
the body length is `bend - 512`; the header is rendered into `hbuf`, `memcpy`'d to
`bbuf + 512 - hn` so it sits directly in front of the body, and the whole thing goes out in
**one `net_send_all`**. Every accepted socket gets `TCP_NODELAY`
(`setsockopt(s, 6, 1, optval, 4)`, where `optval` points at a 4-byte buffer holding the
int 1). Splitting the header and body into two sends gets stalled by **Nagle's algorithm
to the millisecond range per request** — that is exactly why the single-segment send
exists.

**HTTP framing.** `find_hdr_end` scans for `\r\n\r\n` (13/10/13/10) and `req_len = e + 4`;
several requests arriving in one `recv` (keep-alive pipelining) are drained one by one, and
the remainder is slid to the front of the buffer with a **byte loop** (source and
destination may overlap, and overlapping `memcpy` is UB). A partial request simply waits in
the buffer for the next `recv`. If the buffer ever fills (`used == 8192`) the connection is
closed.

**What `/metrics` serves.** Prometheus text format, organised along RED lines
(`render_metrics` in `http_buf.ax`; the metrics-block slot layout is documented in the
comment above it):

```text
# TYPE aoxn_http_requests_total counter
aoxn_http_requests_total{route="/"} <n>
aoxn_http_requests_total{route="/api/json"} <n>
aoxn_http_requests_total{route="/text"} <n>
aoxn_http_requests_total{route="/metrics"} <n>
aoxn_http_requests_total{route="other"} <n>
# TYPE aoxn_http_not_found_total counter
# TYPE aoxn_http_response_bytes_total counter
# TYPE aoxn_http_connections_total counter
# TYPE aoxn_http_connections_active gauge
# TYPE aoxn_http_request_duration_nanoseconds_sum counter
# TYPE aoxn_http_request_duration_nanoseconds_max gauge
# TYPE aoxn_http_uptime_seconds gauge
```

The Content-Type is `text/plain; version=0.0.4; charset=utf-8`. The metrics block is a
112-byte `malloc` whose slots are: 0 start seconds, 8 requests, 16 bytes out, 24 not-found,
32 connections total, 40 connections active, 48 + 8 × route code per-route counts,
88 duration sum (ns), 96 duration max (ns), 104 clock scratch pointer. Each request times
itself by calling `net_now_ns(m)` before and after routing + rendering + sending.
`net_now_ns` lives in the platform layer: `QueryPerformanceCounter` /
`QueryPerformanceFrequency` on Windows, `clock_gettime` on POSIX. **Trap**:
`timespec_get` does not link on Windows (clang/MSVC libs) — do not retry it.

**Platform differences (real, in the POSIX wrapper).** `setsockopt(SO_REUSEADDR)` must run
before `bind`, otherwise a restart or the previous test server's TIME_WAIT connections make
`bind` fail with `EADDRINUSE` on Linux/macOS. **Both constants are platform-specific**: the
level `SOL_SOCKET` is `1` on Linux and `65535` (`0xFFFF`) on macOS/BSD, and the optname
`SO_REUSEADDR` is `2` on Linux and `4` on macOS/BSD. `web/sock_posix.ax` branches on
`target_os()` (`setsockopt(s, 1, 2, opt, 4)` / `setsockopt(s, 65535, 4, opt, 4)`) — passing
Linux's level `1` on macOS **fails silently** and the bug comes right back. Winsock semantics
differ, so **do not** add it on Windows (there it enables port hijacking). Filling `sockaddr_in`
differs too: macOS needs
`store_u8(addr, 0, 16)` + `store_u8(addr, 1, 2)`, Linux the little-endian
`store_u8(addr, 0, 2)`. All of these branches are selected with the compile-time
`target_os()` builtin.

**Threading model.** A single-threaded accept loop that handles one connection at a time
(after EOF/error it calls `closesocket` and decrements the active-connection gauge). So
"zero allocation per request plus a single-segment send" is not only a performance choice,
it is the natural shape of this model.

### 8. Functional tests and CI

`web/loadtest/parity.mjs` (cross-platform functional test; it starts both the Aoxn server
and the Node reference) verifies:

1. **Body parity**: the `/text`, `/api/json` and `/` bodies are **byte-identical** to the
   Node reference (`Buffer.equals`; it prints both byte counts on failure);
2. **404 handling**: an unknown route returns status 404 with a body of exactly
   `not found\n`;
3. **keep-alive**: three sequential requests each return `hello, web\n`;
4. **`/metrics`**: it contains `# TYPE aoxn_http_requests_total counter`,
   `aoxn_http_uptime_seconds` and `aoxn_http_connections_total`, and the counters actually
   reflect this test run (`route="/text"` ≥ 1, 404 count ≥ 1).

It prints `parity: ALL PASS` and exits 0 when everything passes, otherwise exits non-zero
with the failure count.

`.github/workflows/web-bench.yml` runs a matrix on **windows-latest / ubuntu-latest /
macos-14**:

- build the compiler (`cargo build --release`) → build the server
  (Windows: `web/server_win.ax` → `web/server.exe`, `-l ws2_32`; Linux:
  `web/server_posix.ax` → `web/server` with LLVM from `llvm-18-dev`; macOS:
  `/opt/homebrew/opt/llvm@18`);
- **functional parity**: `node web/loadtest/parity.mjs <artifact>`;
- build the Next.js comparison app (`pnpm install --frozen-lockfile` + `pnpm build`);
- install oha 1.16.0 and run a **short reference benchmark** (`BENCH_TRIALS=1`,
  `BENCH_DURATION=5s`), whose table goes into the job summary; finally upload
  `web/loadtest/last-results.json` as the `web-bench-<os>` artifact.

Triggers: push / PR touching `web/**` or the workflow file itself, plus manual
`workflow_dispatch`. **CI runners share CPUs, so those numbers are trend/regression values,
not absolutes**; the official comparison remains the full `bench.mjs` (3 interleaved
trials, median) on dedicated hardware.

### 9. Future plans (these are plans, not the current state)

Everything below comes from `docs/web-platform-plan.md` (the v0 design discussion) and,
apart from the "done" W0 row, **none of it has landed**: **`web/` today is only a benchmark
suite**, not a web framework or a product server.

**Decided directions**

- **TS front end (option A)**: add a TypeScript-syntax lexer/parser that produces the same
  AST/IR as the existing Aoxn pipeline and feeds LLVM directly — the goal is that existing
  `*.ts`/`*.tsx` projects compile and run without changing habits, with Aoxn's Python-style
  syntax retreating to the implementation layer. Two large work items: the TS type system
  (structural typing, unions/intersections/literal types, control-flow narrowing, `any`,
  optionals, tuples) needs a rebuilt check layer, and JS runtime semantics (reference
  semantics for objects, prototype chain, closures, GC) is the biggest piece of "full
  compatibility" and must be phased.
- **Module system**: remove the existing `import "relative-path"` in favour of TS-style
  `import ... from "pkg"` with node_modules-style bottom-up resolution and `package.json`
  `exports`/`main`/`types`; this touches the Rust loader, `selfhost/load.ax` and `stdlib/`
  (a single switch with no dual track — the timing is open decision C).
- **Independent package management (option A2)**: an own manifest (working name
  `aoxn.json`), an own lockfile, a self-hosted registry (protocol aligned with a mature
  subset of the npm Registry API, private scopes and auth) and a resolver that does not
  read node_modules; plus a one-time npm import tool or registry proxy as the ecosystem
  bridge (estimate: the package manager is ~4–6 weeks; the registry and bridge are extra).
- **Styling (option B1)**: `.css` as a first-class resource with a build pipeline
  (bundling/minifying/fingerprinting/cache negotiation); CSS Modules with hashed local
  class names and a typed class-map import; a compatible Tailwind toolchain (JIT output
  feeding the same asset pipeline); a typed CSS-in-TS wrapper as optional second-phase
  sugar; SVG + CSS for graphics first.

**Milestones (the plan's table)**

| Phase | Content | Status |
|---|---|---|
| **W0** | `/metrics` observability + `parity.mjs` + three-platform CI (`web-bench.yml`) + the design doc | **done** |
| **W1** | TS-M1 front end (syntax subset → existing pipeline) + the new module system (import/export) + removing the old `import`; stdlib/selfhost migrated in the same batch | **in progress**: S0 lexer + S1 parser landed (`src/ts/`, end-to-end `.ts` compile+run verified — see [Frontend](Frontend-Lexer-and-Parser.md) §11 and `docs/ts-m1-spec.md`); the module system and the old `import` removal land in S3 |
| **W2** | independent package management (manifest/lockfile/registry client + npm bridge); CSS asset pipeline + CSS Modules + Tailwind; static file service + `Range` + cache headers | unfolds from the A2/B1 decisions |
| **W3** | full benchmark v2 run (complex scenarios + browser rendering metrics + compile times), dedicated Linux/macOS data | depends on W2 |
| **W4** | TS-M2 runtime semantics (objects/closures/GC) → real-project migration acceptance | depends on sample projects |

The TS milestones are: **TS-M1** syntax + static subset (type annotations, `interface`,
`enum`, generics, functions/arrow functions, `let`/`const`, template literals, `class`
fields and methods, import/export) → **TS-M2** JS runtime semantics (object reference
semantics, closures, common array/Map libraries, GC or region allocation to start) →
**TS-M3** full coverage (async/Promise, decorators, namespaces, the common surface of
`lib.dom`/`node.d.ts`).

**The benchmark-v2 metric design (a plan)**: request timing (TTFB, full-response
p50/p95/p99, cold-start first request, keep-alive and new connections reported separately),
**real browser rendering time** (Playwright + CDP tracing for FCP/LCP/TTI, reported
separately from server-side SSR time), and compile duration (small/medium/large ×
cold/warm-cache/incremental; `aoxn build` vs `next build` vs `tsc --noEmit`). Complex
scenarios: S0 empty page (today's baseline), S1 image/text page (20 images, 50KB CSS,
fonts), S2 heavy-resource page (`<video>` streaming with `Range`), S3 static-asset
throughput (images/CSS/fonts, cache headers, gzip/br, ETag), S4 mixed load (70/20/10 for
5 minutes, watching stability and memory). **The supporting capabilities the Aoxn server
still needs**: static file service, `Range`, cache negotiation (ETag/Cache-Control) and
optional gzip. Of the observability four, `/metrics` (RED) has landed; `/healthz` and
structured JSON access logs are planned alongside the TS stack, and request-level traces
(optional OTLP export) are second phase.

**Still open**: the timing of removing the old `import` (C); the npm bridge approach
(one-time import tool vs registry proxy — it also determines how npm tools such as Tailwind
plug in); 2–3 real team projects as TS-M1 acceptance samples; whether to accept
Playwright + Chrome for rendering metrics; and whether official Linux/macOS benchmark data
comes from CI reference values or dedicated machines.

### 10. How this page and the performance page divide the work

**This page is about how fast a server written in Aoxn is**: the suite's structure, routes,
how to run it, functional tests/CI, and the measured throughput/latency/startup/RSS tables
with the Little's-law reading. **[Performance and Benchmarks](Performance-and-Benchmarks.md)
is about how fast the compiler is and how fast the code it generates is**: compile-time
structure, optimization-level tradeoffs, the build cache, the link/startup floor and
measurement discipline. The two pages draw on different sources (mainly
`docs/web-benchmark.md` and `web/loadtest/last-results.json` here). Do not mix them.

---

## 源文件 / Source files

- [web/README.md](../web/README.md) — the suite's own overview, layout, build/run and benchmark commands
- [docs/web-benchmark.md](../docs/web-benchmark.md) — environment & method, the throughput/latency, startup and footprint tables, the Little's-law analysis and the caveats
- [docs/web-platform-plan.md](../docs/web-platform-plan.md) — the plan behind §9: TS front end, module system, package management, styling, observability, benchmark v2, milestones and open decisions
- [web/loadtest/last-results.json](../web/loadtest/last-results.json) — the raw results behind the tables (`readyMs`, `runs[]`, `aggregate[]`)
- [web/loadtest/bench.mjs](../web/loadtest/bench.mjs) — orchestration, tunables, median aggregation, RSS sampling, job-summary output
- [web/loadtest/parity.mjs](../web/loadtest/parity.mjs) — what the functional test actually checks
- [web/serve.ax](../web/serve.ax) — accept loop, framing, keep-alive drain, single-segment send, metrics updates
- [web/http_buf.ax](../web/http_buf.ax) — routing, byte-buffer appends and every response body (including `/metrics`)
- [web/sock_win.ax](../web/sock_win.ax) · [web/sock_posix.ax](../web/sock_posix.ax) — the two socket wrappers, `i32()`, `net_now_ns`
- [web/server_win.ax](../web/server_win.ax) · [web/server_posix.ax](../web/server_posix.ax) — the two entry points
- [web/node-server.mjs](../web/node-server.mjs) — the plain `node:http` reference whose bodies are byte-identical
- [.github/workflows/web-bench.yml](../.github/workflows/web-bench.yml) — the three-platform matrix (parity + short reference benchmark)
- [README.md](../README.md) — the one-paragraph web summary ("26-54x", 173 KB, 5 MB RSS)
- `AGENTS.md` — local, gitignored session notes (verified implementation facts: the POSIX `SO_REUSEADDR` trap, why autocannon was rejected, the same-machine caveat); not a published repository document, so it is referenced as plain text rather than linked
