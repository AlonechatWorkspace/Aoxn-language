# Security Policy

Aoxn is a compiler: it consumes untrusted text (`.ax` sources, import paths) and
produces native machine code that people then execute. Defects that turn that
pipeline into memory corruption or code execution are security issues, and we
want to hear about them privately before they are public.

---

# English

## Supported versions

Aoxn is pre-1.0. Only the latest release line receives security fixes; older
minors do not.

| Version | Supported |
|---|---|
| `0.29.x` (current) | ✅ yes |
| `0.28.x` and earlier | ❌ no — please reproduce on `main` or the latest release |
| `main` (development) | ✅ yes, fixes land here first |

Fix versions are always noted in [`CHANGELOG.md`](CHANGELOG.md). If you need a
fix backported to an older tag, say so in your report and we will discuss it.

## Reporting a vulnerability

**Email `zmjsjsg3@163.com`.** Please do not open a public issue, and do not post
the details in a discussion, chat, or social media before a fix is available.

A useful report contains:

1. **Version** — the first line of `aoxn --help` (or the `version` field in
   `Cargo.toml`), and the commit/tag if you build from source.
2. **Platform** — OS and architecture, clang version, and whether you are on a
   Tier 1 (Windows) or Tier 2 (Linux / macOS) target; see
   [`docs/platform-support.md`](docs/platform-support.md).
3. **Reproduction** — the smallest `.ax` source you can manage, the exact
   command you ran, and observed versus expected behavior. If the problem is in
   generated code, include the generated C (`aoxn c file.ax`, or
   `AOXN_DUMP_C=1`) and say whether it still reproduces with `--O0`.
4. **Impact** — what an attacker gains and what they must already control
   (e.g. "compiling this source executes a shell command" or "the emitted
   function writes past a stack buffer").
5. **Credit** — the name or handle you want used, or a request to stay anonymous.

If you want to encrypt the report, say so in a first email with no details and
we will arrange a channel.

### What to expect

| Stage | Target |
|---|---|
| Acknowledgement of your report | within 3 business days |
| Initial assessment (accepted / not a vulnerability / need more info) | within 10 business days |
| Fix for a confirmed critical issue | next release, or a point release if the line is otherwise closed |
| Coordinated disclosure | 90 days after acknowledgement, or as soon as a fix ships — whichever comes first |

We will keep you updated at each stage and ask before publishing anything that
credits you. If we conclude something is not a vulnerability, we will explain
why, and we are happy to be corrected.

There is no bug bounty. We will credit reporters in the advisory and the
changelog unless you prefer otherwise. One thing we will not do is trade a fix
for silence: if an issue is being exploited in the wild, we will publish with
the information needed to protect users even if the reporter disagrees.

## In scope

- **Memory corruption or code execution in generated code** that the Aoxn source
  did not opt into — wrong `memcpy` sizes, aggregate copy/ABI mistakes,
  out-of-bounds pointer arithmetic in the emitter, string/buffer length
  mishandling, miscompilation that turns a defined program into an unsafe one.
- **Code execution or command injection in the toolchain** — the final link step
  shells out to `clang`, the C backend (the only backend since v0.29.0) compiles
  the generated `.c` file with `clang -c`, and the self-hosted driver calls
  `system("clang ...")`. Crafted file names, `-l`/`-L`
  values, or paths that escape into a shell are in scope.
- **Memory-safety defects inside the compiler itself** — unsafe Rust, use of
  freed or recycled AST nodes, raw-pointer misuse in the C emitter
  (`src/codegen_c.rs`) or the FFI helpers.
- **Build and supply-chain integrity** — the CI workflow, the
  published artifacts, or anything that lets a source file or config influence
  the compiler's own binaries. (The compiler library `src/` has zero external
  dependencies by design, which is also a security property; the separate
  `aoxn-pkg` crate uses pinned, widely used crates from `Cargo.lock`. A PR that
  adds a dependency to `src/` needs a very good reason.)
- **Build-cache poisoning** — `aoxn run`/`aoxn build` execute or copy
  executables from `target/cache` based on a content hash; a way to run
  attacker-controlled code through that path is a vulnerability.
- **The package-manifest reader** (`src/pkg_manifest.rs`, since v0.29.1) — a
  hand-rolled, zero-dependency JSON parser that reads untrusted
  `aox_modules/<pkg>/aoxn.json` files to resolve bare package imports. Like
  the lexer/parser, it consumes untrusted text, so a malformed manifest that
  causes memory corruption or an unbounded hang in the compiler process is in
  scope. (It is deliberately lenient — a bad manifest falls back to the
  directory probe rather than aborting — but a panic or memory-unsafe read is
  still a defect.)
- **The npm bridge** (`crates/aoxn-pkg/src/npm.rs`, since v0.29.5) — imports
  untrusted npm tarballs: extraction rejects `..` traversal, the sha512
  (`dist.integrity`) is verified before unpacking, and only packages with
  an `aoxn.json` root are accepted. A traversal entry that lands outside
  `vendor/`, an integrity bypass, or a path-dep recording that escapes the
  project directory is a vulnerability.
- **Denial of service that is not just "a bad program"** — a small, well-formed
  input that hangs the compiler indefinitely or exhausts memory catastrophically
  is worth reporting; see the note below on where the line is.

## Out of scope (documented behavior)

These are deliberate design decisions, documented in
[`docs/spec.md`](docs/spec.md). Please do not report them as vulnerabilities —
though a bug report about the *documentation* is welcome.

- **Unchecked array indexing and signed integer overflow.** Like C, these are
  undefined behavior in the language contract. Indexing out of bounds or
  overflowing an `int` in Aoxn source is the program's bug, not the compiler's.
- **Raw memory builtins** (`load_i64`, `store_i64`, `load_f64`, `store_f64`,
  `load_u8`, `store_u8`, `as_ptr`, `as_string`). These exist as the
  self-hosting escape hatch and are unsafe by design; unchecked pointer
  arithmetic on their arguments is intentional.
- **Memory growth from string concatenation.** Concat results are never freed
  (immutable strings, no GC yet). It is stated behavior, not a leak bug.
- **Compiler crashes, hangs, or wrong error messages on malformed input.** These
  are bugs — file them with the
  [bug report template](https://github.com/AlonechatWorkspace/Aoxn-language/issues/new?template=bug_report.yml).
  Escalate to a security report only if the failure involves memory corruption
  in the compiler process, code execution, or a wrong-code emission that
  silently makes a valid program unsafe.
- **Vulnerabilities in programs people compile with Aoxn.** Aoxn provides no
  sandbox and no runtime safety net; the compiled program's behavior is the
  program author's responsibility. (The UI toolkit's raw FFI and raw-memory
  helpers are unsafe by design, like everything above.)
- **The X11 server as an untrusted input source.** The `ui_x11.ax` backend
  speaks the X11 protocol to whatever `DISPLAY` points at, so a hostile (or
  merely compromised) X server is in the same position as a hostile terminal
  — it can feed the toolkit crafted events, atoms, selections and fonts. This
  is inherent to speaking X11 and is not a sandbox boundary the toolkit
  claims; the Windows backend has the same property with respect to window
  messages. Defects in *how* the backend parses that input (buffer overruns
  from a crafted event, out-of-bounds writes into the scratch blocks) ARE in
  scope and should be reported.
- **Upstream clang / MSVC defects.** Report those upstream — but do tell
  us if the compiler depends on the broken behavior.
- **Anything requiring an attacker who already controls the machine** or the
  terminal the compiler runs in. The documented `AOXN_*` knobs are
  configuration, not an attack surface — but a crafted value that escapes into
  the link command or poisons the build cache is in scope (see above).

## Safe harbor

We will not pursue or support legal action against researchers who:

- act in good faith and follow this policy,
- test only against their own builds and data,
- avoid privacy violations, data destruction, and disruption of services they do
  not own,
- give us reasonable time to fix the issue before public disclosure, and
- do not use social engineering, physical attacks, or denial-of-service against
  infrastructure (the compiler is a local tool; there is no service to test).

If you are unsure whether something is in scope, ask first — email
`zmjsjsg3@163.com` with just enough detail to describe the area, and we will tell
you how we want it handled.

---
---

# 中文

Aoxn 是一个编译器：它消费不可信的文本（`.ax` 源码、导入路径），产出人们随
后执行的原生机器码。把这条管线变成内存破坏或代码执行的缺陷就是安全问题——
我们希望在公开之前私下收到报告。

## 支持的版本

Aoxn 处于 pre-1.0 阶段：只有最新的版本线接收安全修复，旧的次版本不再修。

| 版本 | 支持情况 |
|---|---|
| `0.29.x`（当前） | ✅ 支持 |
| `0.28.x` 及更早 | ❌ 不支持——请在 `main` 或最新发布上复现 |
| `main`（开发线） | ✅ 支持，修复最先落在这里 |

修复版本永远记在 [`CHANGELOG.md`](CHANGELOG.md)。如需把修复反向移植到旧
tag，请在报告里说明，我们再商量。

## 报告漏洞

**发邮件到 `zmjsjsg3@163.com`。** 请不要开公开 issue，修复可用之前也不要把
细节发到讨论区、聊天或社交媒体。

一份有用的报告包含：

1. **版本** —— `aoxn --help` 的第一行（或 `Cargo.toml` 的 `version` 字段），
   从源码构建的请附 commit/tag。
2. **平台** —— 操作系统与架构、clang 版本，以及你在 Tier 1（Windows）还是
   Tier 2（Linux / macOS）目标上；见 [`docs/platform-support.md`](docs/platform-support.md)。
3. **复现** —— 尽可能小的 `.ax` 源码、确切命令、实测行为与预期行为。若问题
   出在生成代码里，请附生成的 C（`aoxn c file.ax` 或 `AOXN_DUMP_C=1`），并说明
   `--O0` 下是否仍复现。
4. **影响** —— 攻击者得到什么、已须控制什么（例如"编译该源码会执行 shell
   命令"或"生成的函数越过栈缓冲写入"）。
5. **署名** —— 你希望使用的姓名/昵称，或要求匿名。

如需加密报告，请先发一封不含细节的邮件说明，我们再安排通道。

### 你会得到什么

| 阶段 | 目标时限 |
|---|---|
| 确认收到你的报告 | 3 个工作日内 |
| 初步评估（接受 / 不是漏洞 / 需要更多信息） | 10 个工作日内 |
| 确认的关键问题的修复 | 下个发布；若该线已关闭则发补丁版本 |
| 协同披露 | 收到报告起 90 天内，或修复发出即披露——以先到者为准 |

每个阶段我们都会同步进展；发布任何署名内容前会先征得同意。如果我们判定
不是漏洞，会说明理由，也欢迎你反驳。

没有漏洞赏金。除非你另有要求，我们会在公告与变更日志里署名致谢。有一件事
我们不做：用修复换沉默——若漏洞正在被野外利用，即使报告者不同意，我们也会
连同保护用户所需的信息一起公开。

## 范围内

- **生成代码中的内存破坏或代码执行**，且 Aoxn 源码并未主动选择危险行为——
  错误的 `memcpy` 尺寸、聚合复制/ABI 缺陷、发射器中的越界指针运算、
  字符串/缓冲长度处理错误、把良定义程序误编译成不安全程序。
- **工具链中的代码执行或命令注入** —— 最终链接步骤 shell 出去调 `clang`，
  C 后端（v0.29.0 起的唯一后端）会对编译器生成的 `.c` 文件调 `clang -c`，
  自举 driver 调 `system("clang ...")`；构造的文件名、`-l`/`-L`
  值或路径逃逸进 shell 的都在范围内。
- **编译器自身的内存安全缺陷** —— unsafe Rust、释放/回收后 AST 节点的误用、
  C 发射器（`src/codegen_c.rs`）或 FFI 辅助中的原始指针误用。
- **构建与供应链完整性** —— CI 工作流、发布产物，或任何让源码/
  配置影响编译器自身二进制的路径。（编译器库 `src/` 有意保持零外部依赖，
  这本身也是安全属性；独立的 `aoxn-pkg` crate 使用 `Cargo.lock` 锁定的
  成熟第三方 crate。往 `src/` 加依赖的 PR 需要充分理由。）
- **构建缓存投毒** —— `aoxn run`/`aoxn build` 按内容哈希从 `target/cache`
  执行或拷贝可执行文件；能通过该路径运行攻击者控制的代码即为漏洞。
- **包 manifest 读取器**（`src/pkg_manifest.rs`，v0.29.1 起）—— 手写的零依赖
  JSON 解析器，读取不可信的 `aox_modules/<pkg>/aoxn.json` 来解析裸包导入。与
  词法/语法分析器一样消费不可信文本，因此畸形 manifest 若在编译器进程中造成
  内存破坏或无界挂起，属范围内。（解析器刻意宽松——坏 manifest 回退到目录探针
  而非中止——但 panic 或内存不安全读取仍是缺陷。）
- **npm 桥接**（`crates/aoxn-pkg/src/npm.rs`，v0.29.5 起）—— 导入不可信的
  npm tarball：解包拒绝 `..` 目录穿越，sha512（`dist.integrity`）在解包前
  验证，且只接受根目录带 `aoxn.json` 的包。能让文件落到 `vendor/` 之外的
  穿越条目、绕过完整性校验、或把路径依赖记录逃逸出项目目录的行为，均属漏洞。
- **不只是"坏程序"的拒绝服务** —— 一个小的、格式良好的输入让编译器无限挂起
  或灾难性耗尽内存的，值得报告；界线见下文。

## 范围外（文档化行为）

以下是有意的设计决定，记录在 [`docs/spec.md`](docs/spec.md)。请勿作为漏洞
报告——不过针对*文档本身*的缺陷报告欢迎。

- **不检查的数组索引与有符号整数溢出。** 与 C 相同，语言契约中是未定义行为。
  Aoxn 源码里越界索引或 `int` 溢出是程序的 bug，不是编译器的。
- **原始内存内建**（`load_i64`、`store_i64`、`load_f64`、`store_f64`、
  `load_u8`、`store_u8`、`as_ptr`、`as_string`）。它们是自举逃生舱，设计上
  就不安全；对其参数做不检查的指针运算是有意为之。
- **字符串拼接的内存增长。** 拼接结果永不释放（不可变字符串，尚无 GC）。这是
  成文行为，不是泄漏 bug。
- **编译器在畸形输入上的崩溃、挂起或错误信息。** 这些是 bug——用
  [bug report 模板](https://github.com/AlonechatWorkspace/Aoxn-language/issues/new?template=bug_report.yml)
  提交。仅当涉及编译器进程内存破坏、代码执行，或把良定义程序静默输出成不安
  全代码时，才升级为安全报告。
- **人们用 Aoxn 编译出的程序里的漏洞。** Aoxn 不提供沙箱和运行时安全网；编
  译产物的行为由程序作者负责。（UI 工具箱的原始 FFI 与原始内存辅助同理，
  与上述一切一样设计上不安全。）
- **把 X11 服务器当作不可信输入源。** `ui_x11.ax` 后端会与 `DISPLAY` 指向
  的任何 X11 服务器对话，因此恶意的（或已被攻破的）X 服务器与恶意终端处于
  同一位置：它可以向工具箱投喂构造的事件、atom、选区与字体。这是"使用 X11
  协议"本身固有的性质，并非工具箱声称的沙箱边界；Windows 后端对于窗口消
  息也有同样性质。但后端*解析*这些输入时的缺陷（构造事件导致的缓冲区溢
  写、越界写进暂存块）**属于范围内**，请报告。
- **上游 clang / MSVC 的缺陷。** 请报给上游——但若编译器依赖了该坏行
  为，请告知我们。
- **任何已控制编译器所在机器或终端的攻击者才能利用的问题。** 文档化的
  `AOXN_*` 旋钮是配置而非攻击面——但构造的值逃逸进链接命令或毒化构建缓存
  仍在范围内（见上）。

## 安全港

对符合以下条件的研究人员，我们不会追究或支持法律行动：

- 善意行事并遵守本政策；
- 只对自己的构建与数据进行测试；
- 不侵犯隐私、不销毁数据、不干扰不属于自己的服务；
- 公开披露前给我们合理的修复时间；
- 不使用社会工程、物理攻击或对基础设施的拒绝服务（编译器是本地工具，没有
  服务可打）。

不确定某问题是否在范围内，先问——发邮件到 `zmjsjsg3@163.com`，只需足够描述
领域的细节，我们会告诉你希望如何处理。
