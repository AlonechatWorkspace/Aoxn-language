# 开发指南 · Development Guide

> **中文**：从环境准备、克隆构建、跑第一个程序与测试，到仓库导航、改语言的 6 步清单、零依赖与聚合 ABI 纪律、
> Aoxn 源码约定、调试工作流、提交与 PR 政策、AI 协作规则、文档地图，以及如何把本 `wiki/` 目录发布到 GitHub Wiki。
> **English**: From prerequisites, clone-build-run, and the first program and test run, through
> repository navigation,
> the six-step language-change checklist, the zero-dependency and aggregate-ABI disciplines, Aoxn
> source conventions,
> the debug workflow, commit and PR policy, AI-collaboration rules, the documentation map, and how to publish this
> `wiki/` directory to the GitHub Wiki.

## 中文

### 环境准备

Tier 1（Windows x86_64，全支持）需要：

| 需求 | 说明 |
|---|---|
| **Rust**（stable，`x86_64-pc-windows-msvc` host） | `cargo build` / `cargo test` |
| **带 C API 的 LLVM** | 本机验证版本 23.1.0；`build.rs` 按 `AOXN_LLVM_DIR` → `<repo>/LLVM` → 平台默认目录的顺序查找 |
| **clang** | 最后一步链接由它完成，查找顺序：`AOXN_CLANG` → `PATH` → 仓库内 `LLVM\bin\clang.exe` → `C:\Program Files\LLVM\bin\clang.exe` |
| **MSVC Build Tools 2022** | clang 会自动探测它；MSVC host 必需 |

Tier 2（Linux x86_64、macOS x86_64/arm64）需要 LLVM 18（Debian/Ubuntu 的 `llvm-18-dev`，macOS 的
`brew install llvm@18`）并把 `AOXN_LLVM_DIR` 指向它；细节、各平台命令与遗留边界见
[平台支持](Platform-Support.md)。

```powershell
# Windows：winget 装 LLVM（CI 用的就是这一条，含 --silent）
winget install --id LLVM.LLVM --accept-source-agreements --accept-package-agreements --silent
```

```bash
# Tier 2：Linux / macOS
sudo apt-get install -y llvm-18-dev clang-18 libclang-rt-18-dev   # Debian/Ubuntu
brew install llvm@18                                              # macOS
```

Windows 安装包只带 C API（`LLVM-C.lib` / `LLVM-C.dll`）而**不带**逐组件的静态库，所以本项目用手写 FFI
（`src/llvm.rs`），**不要**引入 `inkwell` / `llvm-sys`。

### 克隆、构建、跑第一个程序

```powershell
git clone https://github.com/AlonechatWorkspace/Aoxn-language.git
cd Aoxn-language
cargo build
cargo run -- run examples\hello.ax     # 输出: hello, Aoxn
cargo test                             # 97 个端到端测试
```

```bash
git clone https://github.com/AlonechatWorkspace/Aoxn-language.git
cd Aoxn-language
cargo build
cargo run -- run examples/hello.ax     # 输出: hello, Aoxn
cargo test                             # 97 个端到端测试
```

一个最小程序（写进 `hello.ax`，用 `cargo run -- run hello.ax` 跑）：

```aoxn
# the classic
def main() -> int:
    print("hello, Aoxn")
    return 0
```

日常最常用的几条命令：

```powershell
cargo run -- build examples\fib.ax -o fib.exe   # 产出独立可执行文件（默认 O3）
cargo run -- ir examples\fib.ax                 # dump 优化后的 LLVM IR
cargo run -- run bad.ax --json                  # 诊断以 JSON 输出，便于工具消费
cargo test --test pipeline recursion_fib        # 只跑一个测试
cargo run -- run examples\stdlib_demo.ax        # 演示 import 真实 stdlib
```

注意 `cargo test` 编译并运行真实程序，因此比普通 Rust crate 慢；迭代大输入时可用 `--O1` 把编译时间近乎减半，
但 O3 是默认级别、也是“与 `clang -O3` 同性能”这句承诺所在的级别。

### 仓库导航

| 路径 | 内容 / 何时该动它 |
|---|---|
| `src/lexer.rs` | 词法：token 带行/列/文件，发射 `NEWLINE`/`INDENT`/`DEDENT`（Python 式布局） |
| `src/parser.rs`、`src/ast.rs` | 递归下降解析器与 AST |
| `src/typecheck.rs` | 严格类型规则、`FnSig` 表、泛型单态化 |
| `src/codegen.rs` | 经 LLVM C API 生成 IR → 目标文件（聚合 ABI 与若干硬约束都在这里） |
| `src/llvm.rs` | 手写 LLVM-C FFI（零 crate）；加新符号前先验证它存在 |
| `src/lib.rs`、`src/files.rs` | import 加载、诊断、调用 clang 链接；`files.rs` 是源文件注册表 |
| `src/hashing.rs` | FxHash 风格的快速哈希：内部查找表 + `run`/`build` 缓存键 |
| `src/platform.rs` | 平台抽象（扩展名、栈标志、`target_os_name`） |
| `src/main.rs` | CLI：`build` / `run` / `c`、`--json`、`--O0..--O3`、`-l` / `-L`、构建缓存 |
| `stdlib/stdlib.ax` | 用 Aoxn 写的标准库（泛型 `sort` / `binary_search`、`Vec`、字节缓冲、文件 IO、进程） |
| `examples/*.ax` | 演示程序与基准（hello、fib、primes、vectors、strings、bench_*、stdlib_demo、ffi_llvm） |
| `selfhost/*.ax` | 用 Aoxn 重写的编译器（lexer → parser → typecheck → loader → codegen → driver）与各阶段 demo |
| `web/` | Web 基准套件：Aoxn 写的 HTTP/1.1 服务器（FFI socket）对比 Node/Next.js |
| `tests/pipeline.rs` | 唯一的端到端测试套件（97 个测试） |
| `docs/` | 规范与深度报告（见下面的文档地图） |
| `wiki/` | 本 Wiki 的源文件（扁平 `.md`，发布方式见文末） |
| `CHANGELOG.md` | 版本历史权威；版本 bump 必带条目 |

### 改语言：6 步清单

只要改动触及语法或语义，就必须在**同一次改动**里做完下面六件事：

1. **`docs/spec.md`** —— 规范是契约（normative spec）。规范没写的行为不算语言的一部分，先改规范再改代码。
2. **`src/parser.rs`** 与 **`src/typecheck.rs`** —— 先让它能被解析，再让它能被检查。顺序很重要：类型错误的
   表现必须是**诊断**（`type` 阶段的 `Diag`），而不是崩溃或生成坏代码。
3. **`src/codegen.rs`** —— 发射 IR。到这一步语义已经确定，codegen 只负责把它落到 LLVM 上（注意下面的聚合 ABI
   纪律）。
4. **`tests/pipeline.rs`** —— 加一个在改动前会失败的测试，名字写成它守护的不变式。没有测试的行为改动不会被
   接受（见 [测试与 CI](Testing-and-CI.md)）。
5. **`selfhost/*.ax`** —— 把同一行为移植到 Aoxn 写的编译器，并保持固定点成立。`selfhost_driver_self_compiles`
   会把 Rust 编译器与 Aoxn 编译器对同一程序产出的 **IR 与 COFF 目标文件逐字节比较**，所以任何编译期折叠
   （比如 `target_os()` 的取值、一个新的 builtin）都必须在两个实现里给出同一个结果；这个逐字节测试目前
   **只在 Windows 上执行**，其它平台会自跳过。
6. **`CHANGELOG.md`** —— 版本 bump 总带一条条目；条目里写清可观察的效果与验证方式。

仓库还有几条**故意严格**的家规（想放宽它们属于“语言提案”，不是顺手补丁）：不做隐式 `int`/`float` 转换、不给
已绑定的名字换类型、条件必须是 `bool`、所有路径必须返回、不允许不可达代码、数组索引不检查、raw memory 内建
按设计就是不安全的。

### 零依赖纪律与 LLVM 符号验证

- **零外部 crate 依赖**。需要新的 LLVM 能力时，做法是给 `src/llvm.rs` 加一个 `extern "C"` 声明，并且先证明
  符号真的存在于已安装的库里：

```powershell
findstr /c:"LLVMFoo" "C:\Program Files\LLVM\lib\LLVM-C.lib"
```

- 不要引入 `inkwell` / `llvm-sys`：它们需要完整的静态 LLVM 库，而这个安装包没有。
- 优先用 `LLVMCreateBuilderInContext` 而不是 `LLVMBuilderCreate`：早期 18.1.8 安装包的 `LLVM-C.lib` 里没有
  后者，上下文版本在所有版本上都可用。
- （历史，v0.29.0 前适用）`LLVM-C.dll` 必须与任何链接了 `LLVM-C.lib` 的二进制放在一起；`build.rs` 会把它复制进
  `target/{debug,release}{,/deps,/examples}`，所以 `cargo run` / `cargo test` 不需要额外设 PATH。
- LLVM 的目标注册是**进程全局**的：同一个后端注册两次会让 `LLVMGetTargetFromTriple` 报
  “Cannot choose between targets”，因此 `init_target()` 保持 `Once` 语义（X86 与 AArch64 同时注册不冲突）。

### 聚合 ABI 纪律（改 codegen 前必读）

聚合（struct / 数组）的 ABI 对性能与正确性都敏感，v0.26 之后规则如下：

- **聚合值以指针跨函数边界**：聚合参数传给 callee 的是一个指针，callee 把它 `memcpy` 进自己的局部槽位（保持
  值语义）；聚合返回用 **sret 出参**——隐藏的第一个指针参数 + `ret void`。`extern def` 例外，它保持朴素的 C ABI，
  因为那是 FFI 边界。
- **拷贝永远是显式 `memcpy`**，绝不整体 `load`/`store` 一个聚合：曾经的 500 字节 40 字段结构体整体 load 值
  ~15s 的 instcombine 与 14–44s 的指令选择（自举编译器 codegen 从 32.0s 降到 3.3s 就是去掉它换来的）。
- **`memcpy` 的大小必须是纯整数常量**：用 `LLVMStoreSizeOfType`，并且模块的 data layout 必须在发射任何 IR
  **之前**建好；`LLVMSizeOf` 返回的是 `ptrtoint(gep)` 常量表达式，成千上万个这样的常量会拖垮整个 pass 流水线。
- 动手前先读 `src/codegen.rs` 里 `type_size`、`emit_aggregate_ptr`、`copy_value` 的注释；可观察的规则写在
  `docs/spec.md`。原理与更多 LLVM 细节见 [代码生成与 LLVM](Codegen-and-LLVM-FFI.md)。

### Aoxn 源码约定

- **4 空格缩进**；**`#` 才是注释**（`//` 是整除运算符）；**语句不加分号**（`;` 只出现在 `[T; N]` 数组类型里）；
  语言源文件用 **`.ax`** 扩展名。
- **绝不写 UTF-8 BOM**：Aoxn 按字节读源码，`EF BB BF` 会在 1:1 报 “unexpected character”。PowerShell 5.1 的
  `-Encoding UTF8` 会加 BOM——用编辑器或 Write 工具写文件，若用脚本重写过 `.ax` 就按字节剥掉前三个字节。
- **fixture 用 ASCII**（包括目录名）：自举 loader 用窄字符 `fopen` 打开文件，非 ASCII 路径在自举路径上会失败，
  而 Rust 编译器不受影响。
- Rust 侧：提交前 `cargo fmt`，保持 warning 数为零。

### 调试工作流

按这个顺序排查，几乎不会走弯路（完整环境变量表见 [命令行与工具链](CLI-and-Tooling.md)）：

1. **先看阶段耗时**：`AOXN_TIME=1` 打印 lex / parse / typecheck / codegen / link 各阶段，以及 codegen 的子阶段
   （`cg.build`、`cg.verify`、`cg.target`、`cg.passes`、`cg.isel`）。先知道时间花在哪、失败在哪。
2. **再看 IR**：`AOXN_DUMP_IR=1` 把 verify 之前的 IR 打到 stderr（配置错误、类型/ABI 问题常常一眼可见）；
   想看优化后的 IR 用 `cargo run -- ir file.ax`。
3. **要定位到函数**：`AOXN_TC_TRACE=1` / `AOXN_CG_TRACE=1` 逐函数打印 marker，编译器崩溃时用它锁定是哪个函数。
4. **要机器可读诊断**：`--json` 输出 `{"ok":false,"errors":[{"stage","file","line","col","message"}]}`，stage 取值
   `lex | parse | type | internal | link | io`；编译器内部失败必须走 `internal` 诊断而不是 panic。

```powershell
$env:AOXN_TIME = "1"; cargo run -- run examples\fib.ax
$env:AOXN_DUMP_IR = "1"; cargo run -- build examples\fib.ax -o fib.exe
$env:AOXN_TC_TRACE = "1"; $env:AOXN_CG_TRACE = "1"; cargo run -- build examples\fib.ax
```

```bash
AOXN_TIME=1 cargo run -- run examples/hello.ax
AOXN_DUMP_IR=1 cargo run -- build examples/fib.ax -o fib
AOXN_TC_TRACE=1 AOXN_CG_TRACE=1 cargo run -- build examples/fib.ax
```

常见症状：“编译产物找不到”多半是平台扩展名问题（非 Windows 没有 `.exe`、目标文件是 `.o`）；
`cannot find -lLLVM-C` 在 Linux 上属预期，应走 `platform::llvm_link_name()`。更多排错项见
[测试与 CI](Testing-and-CI.md) 与 [常见问题与排错](Troubleshooting-FAQ.md)。

### 提交与 PR 政策

**本仓库是全量提交（full commits）**：每个提交都包含整个工作树，用 `git add -A`（**不是** `git add <file>`
挑文件，也**不是** `git add .`），然后提交、核对、推送：

```powershell
git add -A
git commit -m "codegen: ..."
git status          # 核对没有遗漏
git push
```

- 适用于源码、测试、文档、`selfhost/` 的 Aoxn 源码、CI 配置，以及本次会话新建的任何文件。
- 提交前删掉自己创建的临时探针文件（scratch `.ps1` / `.ax` / 日志），别让它们进历史；**不要**删用户写的文件。
- `AGENTS.md` 被 gitignore，因此从不参与提交（它是本地会话笔记，不是仓库文档）。
- 工作直接在 `main` 上推进并推送，CI 会在四个平台上跑每次改动。

PR 要求：

1. 一个 PR 一个逻辑改动；无关的清理另开一个。
2. 填 [`.github/PULL_REQUEST_TEMPLATE.md`](../.github/PULL_REQUEST_TEMPLATE.md)：Summary、Related issues、
   Type of change、How this was verified（粘贴**实际命令与结果**）、Compatibility and risk、Checklist。
3. Windows（Tier 1）与 Tier 2 的 CI 全绿才能进 review；本地 `cargo test` 也要过，平台相关的改动请把结果贴出来。
4. **性能声明必须带数字与方法**（机器、冷/热、best of N）；基准用 `--release`，因为 dev profile 是
   `opt-level = 1`。
5. 预期 review 会盯着规范、测试与固定点——那正是这个项目的重点。

### AI 协作规则

AI 参与是被明确欢迎的（`--json` 诊断与严格语义本来就是为“让机器生成的代码可验证”而设计的），但有四条硬规矩：

- **披露**：在 PR 描述里写明（例如 “generated with <tool>, reviewed by me”）；PR 模板里有对应的勾选项。
- **逐条核实**：生成的补丁必须真的能 build、真的能过 `cargo test`，不得编造 LLVM-C 符号、基准数字或规范文本；
  要跑命令，而不是复述工具声称会发生什么。
- **diff 可 review**：不要大范围重排格式、不要重写一个几乎没碰到的文件、不要倾倒未经审阅的生成文件。
- **责任归提交者**：如果 reviewer 在 AI 写的代码里发现问题，那是你的补丁，不是工具的。

### 文档地图

| 文件 | 内容 | 何时看 / 与 wiki 的分工 |
|---|---|---|
| `docs/spec.md` | 语言规范（语法、类型、语义、工具契约、平台声明） | 权威契约；wiki 做导览，冲突时以它为准 |
| `docs/selfhost.md` | 自举可行性评估与分阶段计划 | 历史评估，**部分内容已过时**（进度以 CHANGELOG 与 `selfhost/` 代码为准） |
| [docs/platform-support.md](../docs/platform-support.md) | 平台支持调查报告（B1–B4、S1–S3 阻塞项 + §7 的 T1–T5） | 平台问题的取证原文；摘要见 [平台支持](Platform-Support.md) |
| `docs/platform-migration-plan.md` | M0–M5 迁移计划、风险登记册、测试矩阵 | 计划原文；wiki 只给摘要 |
| `docs/optimization-report.md` | 编译速度调查报告（F1–F6，含实测数据与方法） | 优化决策的依据；改性能前先读 |
| `docs/p2-compiler-performance.md` | 更早一轮编译器性能方案（`AOXN_TIME` 度量与 A1/A2/A5 等条目） | 历史方案；部分条目已落地 |
| `docs/web-benchmark.md` | Web 基准的实测结果 | 结论与数据；方向见 `docs/web-platform-plan.md` |
| `docs/web-platform-plan.md` | Web 平台方向（TS 前端、包管理、CSS 兼容） | 设计决策原文 |
| `CHANGELOG.md` | 版本历史（含 0.26.x 的平台化与性能条目） | **版本历史的权威**；README 的 Status 段已过时（仍写 v0.7 / 70 tests），不要照抄 |
| `wiki/`（本目录） | 面向“上手 + 原理”的连续叙述与交叉链接 | wiki 给可读的导览与最小示例，深度报告留在 `docs/` 原文 |

### 把 `wiki/` 发布到 GitHub Wiki

GitHub Wiki 的页面是**扁平文件**：没有子目录，文件名即页面名。三个名字有特殊含义：`Home.md`（首页）、
`_Sidebar.md`（侧边栏导航）、`_Footer.md`（页脚）。本目录已经包含这三者与各主题页。

发布方式就是一次普通的 git 推送：

```bash
git clone https://github.com/AlonechatWorkspace/Aoxn-language.wiki.git
cp wiki/*.md Aoxn-language.wiki/
cd Aoxn-language.wiki
git add -A
git commit -m "wiki: sync pages"
git push
```

两点说明：

- 页内互链写成 `Page-Name.md`（例如 `[测试与 CI](Testing-and-CI.md)`），这在 GitHub Wiki 上同样可用——Wiki
  会把同目录的 `.md` 解析成对应页面。不要链接其它页面的锚点。
- 指向仓库源码的链接在页内写成 `../` 相对路径（例如 `../src/platform.rs`），这在**本仓库内**查看时有效；
  发布到 Wiki 之后 Wiki 仓库里并没有源码，需要按需改写为绝对 URL，例如
  `https://github.com/AlonechatWorkspace/Aoxn-language/blob/main/src/platform.rs`。

## English

### Prerequisites

Tier 1 (Windows x86_64, fully supported) needs:

| Requirement | Notes |
|---|---|
| **Rust** (stable, `x86_64-pc-windows-msvc` host) | `cargo build` / `cargo test` |
| **LLVM with the C API** | 23.1.0 verified locally; `build.rs` searches `AOXN_LLVM_DIR` → `<repo>/LLVM` → platform default directories |
| **clang** | the final link step shells out to it; lookup order `AOXN_CLANG` → `PATH` → repo-local `LLVM\bin\clang.exe` → `C:\Program Files\LLVM\bin\clang.exe` |
| **MSVC Build Tools 2022** | clang auto-detects them; required for the MSVC host |

Tier 2 (Linux x86_64, macOS x86_64/arm64) needs LLVM 18 (`llvm-18-dev` on Debian/Ubuntu, `brew install llvm@18` on
macOS) with `AOXN_LLVM_DIR` pointing at it; details, per-platform commands and the remaining boundaries are in
[Platform Support](Platform-Support.md).

```powershell
# Windows: installs LLVM via winget (exactly what CI does, --silent included)
winget install --id LLVM.LLVM --accept-source-agreements --accept-package-agreements --silent
```

```bash
# Tier 2: Linux / macOS
sudo apt-get install -y llvm-18-dev clang-18 libclang-rt-18-dev   # Debian/Ubuntu
brew install llvm@18                                              # macOS
```

The Windows installer ships the C API (`LLVM-C.lib` / `LLVM-C.dll`) but **not** the per-component static libraries,
which is why this project keeps hand-written FFI (`src/llvm.rs`) and never pulls in `inkwell` / `llvm-sys`.

### Clone, build, run your first program

```powershell
git clone https://github.com/AlonechatWorkspace/Aoxn-language.git
cd Aoxn-language
cargo build
cargo run -- run examples\hello.ax     # prints: hello, Aoxn
cargo test                             # 97 end-to-end tests
```

```bash
git clone https://github.com/AlonechatWorkspace/Aoxn-language.git
cd Aoxn-language
cargo build
cargo run -- run examples/hello.ax     # prints: hello, Aoxn
cargo test                             # 97 end-to-end tests
```

A minimal program (write it to `hello.ax` and run `cargo run -- run hello.ax`):

```aoxn
# the classic
def main() -> int:
    print("hello, Aoxn")
    return 0
```

The commands you will use daily:

```powershell
cargo run -- build examples\fib.ax -o fib.exe   # standalone executable (O3 by default)
cargo run -- ir examples\fib.ax                 # dump the optimized LLVM IR
cargo run -- run bad.ax --json                  # diagnostics as JSON, for tooling
cargo test --test pipeline recursion_fib        # run a single test
cargo run -- run examples\stdlib_demo.ax        # imports the real stdlib
```

Note that `cargo test` compiles and runs real programs, so it is slower than a typical Rust crate;
while iterating on a
large input, `--O1` roughly halves compile time, but O3 remains the default and the level the "parity with
`clang -O3`" promise refers to.

### Repository navigation

| Path | Contents / when to touch it |
|---|---|
| `src/lexer.rs` | tokens with line/col/file, emitting `NEWLINE`/`INDENT`/`DEDENT` (Python-style layout) |
| `src/parser.rs`, `src/ast.rs` | recursive-descent parser and AST |
| `src/typecheck.rs` | strict type rules, the `FnSig` table, generic monomorphization |
| `src/codegen.rs` | LLVM IR through the C API → object file (the aggregate ABI and its hard rules live here) |
| `src/llvm.rs` | hand-written LLVM-C FFI (zero crates); verify a symbol exists before using it |
| `src/lib.rs`, `src/files.rs` | import loading, diagnostics, the clang link; `files.rs` is the source-file registry |
| `src/hashing.rs` | FxHash-style fast hashing: internal lookup tables plus the `run`/`build` cache key |
| `src/platform.rs` | platform abstraction (extensions, stack flag, LLVM link-name probing, `target_os_name`) |
| `src/main.rs` | CLI: `build` / `run` / `ir`, `--json`, `--O0..--O3`, `-l` / `-L`, the build cache |
| `build.rs` | locate and link LLVM (platform-branched; see [Platform Support](Platform-Support.md)) |
| `stdlib/stdlib.ax` | the standard library written in Aoxn (generic `sort` / `binary_search`, `Vec`, byte buffers, file IO, processes) |
| `examples/*.ax` | demo programs and benchmarks (hello, fib, primes, vectors, strings, bench_*, stdlib_demo, ffi_llvm) |
| `selfhost/*.ax` | the compiler rewritten in Aoxn (lexer → parser → typecheck → loader → codegen → driver) plus per-stage demos |
| `web/` | the web benchmark suite: an Aoxn HTTP/1.1 server (FFI sockets) versus Node/Next.js |
| `tests/pipeline.rs` | the single end-to-end suite (97 tests) |
| `docs/` | the spec and the deep reports (see the documentation map below) |
| `wiki/` | the sources of this wiki (flat `.md` files; publishing is described at the end) |
| `CHANGELOG.md` | the authoritative version history; every version bump carries an entry |

### Changing the language: the six-step checklist

Whenever a change touches grammar or semantics, all six items happen in the **same** change:

1. **`docs/spec.md`** — the normative spec *is* the contract. Behavior that is not in the spec is not part of the
   language, so update the spec before the code.
2. **`src/parser.rs`** and **`src/typecheck.rs`** — make it parse, then make it check. The order
   matters: a type error
   must surface as a **diagnostic** (a `type`-stage `Diag`), never as a crash or bad code.
3. **`src/codegen.rs`** — emit the IR. By this point the semantics are settled and codegen only has
   to lower them onto
   LLVM (mind the aggregate-ABI discipline below).
4. **`tests/pipeline.rs`** — add a test that would have failed before the change, named like the
   invariant it protects.
   Behavior changes without tests are not accepted (see [Testing & CI](Testing-and-CI.md)).
5. **`selfhost/*.ax`** — port the same behavior to the Aoxn-written compiler and keep the fixed point intact.
   `selfhost_driver_self_compiles` compares the **IR and the COFF object file** produced for the
   same program by the
   Rust compiler and by the Aoxn-built compiler byte for byte, so every compile-time fold (the value
   of `target_os()`,
   a new builtin) must agree in both implementations. That byte-exact test currently runs **on Windows only** and
   self-skips elsewhere.
6. **`CHANGELOG.md`** — a version bump always comes with an entry; state the observable effect and
   how it was verified.

The repository also has a few **deliberately strict** house rules (relaxing them is a language proposal, not a
drive-by patch): no implicit `int`/`float` conversions, no re-typing a bound name, `bool`
conditions only, all paths
must return, no unreachable code, array indexing is unchecked, and the raw-memory builtins are unsafe by design.

### Zero-dependency discipline and LLVM symbol verification

- **Zero external crate dependencies.** A new LLVM capability means adding an `extern "C"` declaration to
  `src/llvm.rs` *and* proving the symbol really exists in the installed library first:

```powershell
findstr /c:"LLVMFoo" "C:\Program Files\LLVM\lib\LLVM-C.lib"
```

- Do not introduce `inkwell` / `llvm-sys`: they need the full static LLVM libraries, which this install does not
  have.
- Prefer `LLVMCreateBuilderInContext` over `LLVMBuilderCreate`: the 18.1.8 installer's `LLVM-C.lib` was missing the
  latter, while the context variant works on every version.
- `LLVM-C.dll` must sit next to any binary that links `LLVM-C.lib`; `build.rs` copies it into
  `target/{debug,release}{,/deps,/examples}`, so `cargo run` / `cargo test` need no PATH setup.
- LLVM target registration is **process-global**: registering the same backend twice makes
  `LLVMGetTargetFromTriple` fail with "Cannot choose between targets", so `init_target()` keeps its
  `Once` semantics
  (registering X86 and AArch64 together does not conflict).

### Aggregate ABI discipline (read before touching codegen)

The aggregate (struct / array) ABI is sensitive for both performance and correctness; since v0.26 the rules are:

- **Aggregate values cross function boundaries as a pointer**: an aggregate argument is passed as a
  pointer that the
  callee `memcpy`s into its own local slot (preserving value semantics), and aggregate returns use an **sret
  out-pointer** — a hidden first pointer parameter plus `ret void`. `extern def` is the exception
  and keeps the plain
  C ABI, because that is the FFI boundary.
- **Copies are always explicit `memcpy`s**; never `load`/`store` a whole aggregate: a single
  500-byte, 40-field struct
  load once cost ~15s of instcombine and 14–44s of instruction selection (removing it is what took the self-hosting
  compiler's codegen from 32.0s to 3.3s).
- **`memcpy` sizes must be plain integer constants**: use `LLVMStoreSizeOfType`, with the module data layout
  established *before* any IR is emitted; `LLVMSizeOf` yields a `ptrtoint(gep)` constant expression,
  and thousands of
  those dominate the whole pass pipeline.
- Before editing emission, read the comments on `type_size`, `emit_aggregate_ptr` and `copy_value` in
  `src/codegen.rs`; the observable rules live in `docs/spec.md`. For the underlying LLVM details see
  [Codegen & LLVM](Codegen-and-LLVM-FFI.md).

### Aoxn source conventions

- **4-space indentation**; **`#` is the comment marker** (`//` is integer division); **no statement-terminating
  semicolons** (`;` appears only inside `[T; N]` array types); language sources use the **`.ax`** extension.
- **Never write a UTF-8 BOM**: Aoxn reads sources byte-wise, so `EF BB BF` produces "unexpected character" at 1:1.
  PowerShell 5.1's `-Encoding UTF8` adds one — write files with an editor or the Write tool, and if
  a script rewrote a
  `.ax` file, strip the first three bytes.
- **Keep fixtures ASCII** (directory names included): the self-hosted loader opens files with narrow `fopen`, so
  non-ASCII paths fail on the self-hosted path, while the Rust compiler is unaffected.
- On the Rust side: run `cargo fmt` before committing and keep the warning count at zero.

### Debug workflow

Work through this order and you will rarely waste a step (the complete environment-variable table is in
[CLI & Tooling](CLI-and-Tooling.md)):

1. **Look at the stage timings first**: `AOXN_TIME=1` prints lex / parse / typecheck / codegen /
   link plus the codegen
   sub-phases (`cg.build`, `cg.verify`, `cg.target`, `cg.passes`, `cg.isel`). You immediately learn where time goes
   and where the failure sits.
2. **Then look at the IR**: `AOXN_DUMP_IR=1` dumps pre-verify IR to stderr (misconfiguration and
   type/ABI problems are
   often obvious there); for optimized IR use `cargo run -- ir file.ax`.
3. **To pin down a function**: `AOXN_TC_TRACE=1` / `AOXN_CG_TRACE=1` print per-function markers — invaluable for
   locating a compiler crash.
4. **For machine-readable diagnostics**: `--json` emits
   `{"ok":false,"errors":[{"stage","file","line","col","message"}]}` with the stage one of
   `lex | parse | type | internal | link | io`; internal compiler failures must surface as
   `internal` diagnostics, not
   panics.

```powershell
$env:AOXN_TIME = "1"; cargo run -- run examples\fib.ax
$env:AOXN_DUMP_IR = "1"; cargo run -- build examples\fib.ax -o fib.exe
$env:AOXN_TC_TRACE = "1"; $env:AOXN_CG_TRACE = "1"; cargo run -- build examples\fib.ax
```

```bash
AOXN_TIME=1 cargo run -- run examples/hello.ax
AOXN_DUMP_IR=1 cargo run -- build examples/fib.ax -o fib
AOXN_TC_TRACE=1 AOXN_CG_TRACE=1 cargo run -- build examples/fib.ax
```

A common symptom: "the compiler artifact is missing" is usually a platform extension problem (off
Windows there is no
`.exe` and objects are `.o`); `cannot find -lLLVM-C` is expected on Linux and the correct path is
`platform::llvm_link_name()`. More entries are in [Testing & CI](Testing-and-CI.md) and
[Troubleshooting FAQ](Troubleshooting-FAQ.md).

### Commit and PR policy

**This repository uses full commits**: every commit contains the whole working tree, staged with `git add -A`
(**not** `git add <file>` cherry-picking, and **not** `git add .`), then committed, verified and pushed:

```powershell
git add -A
git commit -m "codegen: ..."
git status          # verify nothing is left behind
git push
```

- This applies to source, tests, docs, the Aoxn sources under `selfhost/`, CI config, and any file
  created during the
  session.
- Delete throwaway probe files you created (scratch `.ps1` / `.ax` / logs) before committing so they never land in
  history; do **not** delete files authored by the user.
- `AGENTS.md` is gitignored and therefore never part of a commit (it is local session context, not repository
  documentation).
- Work happens on `main` and is pushed, so CI exercises every change on all four platforms.

Pull requests:

1. One logical change per PR; keep unrelated cleanups in a separate one.
2. Fill in [`.github/PULL_REQUEST_TEMPLATE.md`](../.github/PULL_REQUEST_TEMPLATE.md): Summary,
   Related issues, Type of
   change, How this was verified (paste the **actual commands and results**), Compatibility and risk, Checklist.
3. Green CI on Windows (Tier 1) and the Tier 2 targets is required before review; `cargo test` must
   pass locally too,
   and platform-specific work should paste its results.
4. **Performance claims need numbers and a method** (machine, warm/cold, best of N); benchmark with `--release`,
   because the dev profile is `opt-level = 1`.
5. Expect review to push on the spec, the tests and the fixed point — that is the point of the project.

### AI-collaboration rules

AI participation is explicitly welcome (the `--json` diagnostics and the strict semantics exist to make
machine-generated code verifiable), but four rules are hard:

- **Disclose it**: say so in the PR description (e.g. "generated with \<tool\>, reviewed by me");
  the PR template has a
  matching checkbox.
- **Verify every claim**: a generated patch must actually build, actually pass `cargo test`, and must not invent
  LLVM-C symbols, benchmark numbers or spec text. Run the commands; never report what the tool said would happen.
- **Keep the diff reviewable**: no bulk reformatting, no rewriting a file your change barely touches, no unreviewed
  dump of generated files.
- **You own the result**: if a reviewer finds a problem in AI-written code, it is your patch, not the tool's.

### Documentation map

| File | Contents | When to read it / how it splits work with the wiki |
|---|---|---|
| `docs/spec.md` | the language spec (grammar, typing, semantics, tooling contract, platform declaration) | the authoritative contract; the wiki is a guided tour, and the spec wins on conflicts |
| `docs/selfhost.md` | self-hosting feasibility assessment and staged plan | a historical assessment, **partly outdated** (progress: follow CHANGELOG and the `selfhost/` sources) |
| [docs/platform-support.md](../docs/platform-support.md) | the platform-support survey (blockers B1–B4, S1–S3, plus §7's T1–T5) | the primary evidence for platform issues; the summary is [Platform Support](Platform-Support.md) |
| `docs/platform-migration-plan.md` | the M0–M5 migration plan, risk register, test matrix | the plan itself; the wiki only summarizes |
| `docs/optimization-report.md` | the compile-speed survey (F1–F6, with measurements and method) | the basis for optimization decisions; read it before touching performance |
| `docs/p2-compiler-performance.md` | an earlier compiler-performance plan (`AOXN_TIME` measurement, items A1/A2/A5) | historical; some items have landed |
| `docs/web-benchmark.md` | measured web benchmark results | the results and data; the direction lives in `docs/web-platform-plan.md` |
| `docs/web-platform-plan.md` | the web platform direction (TS front end, package management, CSS compatibility) | the design decisions |
| `CHANGELOG.md` | version history (including the 0.26.x platform and performance entries) | **the authoritative version history**; README's Status section is stale (still says v0.7 / 70 tests) — do not copy it |
| `wiki/` (this directory) | continuous "getting started + how it works" prose with cross-links | the wiki is the readable tour with minimal examples; the deep reports stay in `docs/` |

### Publishing `wiki/` to the GitHub Wiki

GitHub Wiki pages are **flat files**: no subdirectories, and the file name is the page name. Three
names are special:
`Home.md` (the landing page), `_Sidebar.md` (navigation) and `_Footer.md` (footer). This directory already contains
those three plus the topic pages.

Publishing is an ordinary git push:

```bash
git clone https://github.com/AlonechatWorkspace/Aoxn-language.wiki.git
cp wiki/*.md Aoxn-language.wiki/
cd Aoxn-language.wiki
git add -A
git commit -m "wiki: sync pages"
git push
```

Two notes:

- In-page links are written as `Page-Name.md` (for example `[Testing & CI](Testing-and-CI.md)`), which works on the
  GitHub Wiki as well — the wiki resolves same-directory `.md` files to the corresponding pages. Do
  not link anchors
  of other pages.
- Links to repository sources are written as `../` relative paths (for example
  `../src/platform.rs`), which works when
  browsing **inside this repository**; after publishing to the wiki there is no source tree in the
  wiki repository, so
  rewrite them to absolute URLs as needed, e.g.
  `https://github.com/AlonechatWorkspace/Aoxn-language/blob/main/src/platform.rs`.

---

## 源文件 / Source files

- [CONTRIBUTING.md](../CONTRIBUTING.md) — prerequisites, build/test commands, repository layout, the
  six-step language
  checklist, code conventions, tests, commit/PR policy, AI-assisted contributions.
- [.github/PULL_REQUEST_TEMPLATE.md](../.github/PULL_REQUEST_TEMPLATE.md) — the sections and
  checklist a PR must fill
  in (verification commands, compatibility and risk, AI disclosure).
- [Cargo.toml](../Cargo.toml) — v0.26.3, the `aoxn` bin/lib targets, `[profile.dev] opt-level = 1`, release LTO.
- [build.rs](../build.rs) — LLVM discovery, link name, rpath, `LLVM-C.dll` staging.
- [src/platform.rs](../src/platform.rs) — the platform helpers code must route through.
- [src/hashing.rs](../src/hashing.rs) — the FxHash-style hasher used by internal tables and the build-cache key.
- [src/main.rs](../src/main.rs) — CLI surface, `--O*` levels, the `run`/`build` content-hash cache
  (`cache_key`, `cache_dir`, `prune_cache`).
- [src/lib.rs](../src/lib.rs) — diagnostics stages, clang lookup, `dependency_files`, the aggregate link flags.
- [tests/pipeline.rs](../tests/pipeline.rs) — the suite, its helpers, and the fixed-point comparison the checklist
  refers to.
- [docs/spec.md](../docs/spec.md) — tooling contract, platform support/target_os, strict semantics.
- [docs/selfhost.md](../docs/selfhost.md), [docs/optimization-report.md](../docs/optimization-report.md),
  [docs/p2-compiler-performance.md](../docs/p2-compiler-performance.md),
  [docs/web-benchmark.md](../docs/web-benchmark.md), [docs/web-platform-plan.md](../docs/web-platform-plan.md) —
  the documentation map entries.
- [CHANGELOG.md](../CHANGELOG.md) — 0.26.0–0.26.3 entries (aggregate ABI, platform fixes, cache, dev profile).
- [README.md](../README.md) — read for its stale Status section (v0.7 / 70 tests) referenced in the
  documentation map.
- `AGENTS.md` — 本地会话笔记（被 gitignore，不随仓库发布，故此处不设链接）：全量提交政策、LLVM/clang 查找
  顺序、聚合 ABI 与调试开关等事实的来源。
