# Aoxn

**Aoxn** is an AI-native, statically typed, ahead-of-time compiled programming
language. Python-style syntax on the surface, C++-class native performance
underneath: programs compile through LLVM (O3) straight to machine code.

```Aoxn
def sort[T, N](arr: [T; N]) -> [T; N]:      # generic, monomorphized
    result = arr
    for i in range(N):
        for j in range(N - 1 - i):
            if result[j] > result[j + 1]:
                t = result[j]
                result[j] = result[j + 1]
                result[j + 1] = t
    return result

def main() -> int:
    nums = [5, 3, 8, 1]
    print(f"sorted: {sort(nums)[0]}")   # sorted: 1
    print(sort(["pear", "apple"])[0])   # apple
    return 0
```

---

# English

## Why Aoxn

- **Python-style syntax** — indentation blocks, `def` / `elif` / `pass`,
  `#` comments, `x = 5` type inference, `and` / `or` / `not`, `[0] * n`
- **Native speed** — LLVM O3 backend; measured at parity with `clang -O3`
  (benchmarks below)
- **Generics** — `def sort[T, N](arr: [T; N])` monomorphized per call site;
  no boxing, no runtime overhead
- **AI-native tooling** — every diagnostic can be emitted as structured JSON
  (`--json`), the optimized IR is dumpable (`aoxn ir`), and the semantics are
  deliberately strict and deterministic so AI-generated code is verifiable
- **Value semantics** — arrays, structs, and strings copy by value; no hidden
  references; array indexing is unchecked, C-style

## Quick start

Prerequisites (Windows, Tier 1): Rust (msvc host), LLVM (C API), clang, MSVC
Build Tools. `build.rs` locates LLVM automatically (`AOXN_LLVM_DIR`
overrides). Linux and macOS are Tier 2 and fully green in CI (see
[`docs/platform-support.md`](docs/platform-support.md)).

```powershell
cargo build
cargo run -- run examples\hello.ax
```

Emit a standalone native executable:

```powershell
cargo run -- build examples\fib.ax -o fib.exe
.\fib.exe
```

More: `cargo run -- ir examples\fib.ax` dumps the optimized LLVM IR;
`cargo run -- run examples\primes.ax --json` emits machine-readable
diagnostics; `--cpu native` targets the host CPU (AVX2 & co.) for maximum
speed — the default generic CPU keeps compiled output reproducible across
machines. Since v0.27.1 there is also an experimental **C-emitting backend**:
`--backend c` compiles through generated C + the same clang toolchain instead
of LLVM IR (byte-identical example output, runtime within ±10%, +6% compile
time on a 7k-line input — details in
[`docs/llvm-independence-report.md`](docs/llvm-independence-report.md)); the
default backend stays LLVM.

Optimization levels, for when compile time matters more than runtime speed:

```powershell
cargo run -- run examples\fib.ax --O1     # default<O1>: ~2x faster compile on
                                          # large inputs; O3 stays the default
cargo run -- build examples\fib.ax --O0   # no IR pipeline + fast-isel backend
```

`aoxn run` and `aoxn build` cache their output by program content hash +
compiler + options (`target/cache`), so re-running or re-building unchanged
sources skips compile and link entirely (measured ~1.1s → ~85ms for
`examples/hello.ax`). Set `AOXN_NO_CACHE=1` to disable it or
`AOXN_CACHE_DIR=<dir>` to relocate it.

## Language tour

```Aoxn
# arrays, structs, strings — all value types
struct Particle:
    x: float
    y: float
    mass: float

def energy(p: Particle) -> float:
    return (p.x * p.x + p.y * p.y) * p.mass

def main() -> int:
    ps: [Particle; 1000] = [Particle(x=1.5, y=0.5, mass=2.0)] * 1000

    total = 0.0
    i = 0
    while i < len(ps):
        total = total + energy(ps[i])
        i = i + 1

    print(total)
    print("a" < "b")     # true — strings compare byte-wise
    return 0
```

Types are strict and every expression is checked: `int`/`float` never mix
implicitly, conditions must be `bool`, array indexing is unchecked (C-style),
and every function must return a value on all paths. Strictness is a feature:
the guarantees are simple enough for a machine to reason about.

## Standard library and the UI toolkit

`stdlib/stdlib.ax` is written in Aoxn itself: generic `sort` /
`binary_search` / aggregation, a growable `Vec`, byte buffers, file IO and
process spawning.

Since v0.27.0 the stdlib ships a **Qt-flavored, immediate-mode UI toolkit**
in pure Aoxn on raw Win32/GDI FFI — buttons, checkboxes, sliders, progress
bars, labels, panels, light/dark palettes, UTF-8/emoji text, polled input.
Widgets are plain functions called every frame and the application owns all
state, which is what fits a language without callbacks (yet):

```Aoxn
import * from "../stdlib/ui_win.ax"

def main() -> int:
    c = ui_init("Hello", 640, 480)
    while c.open:
        c = ui_frame(c)
        if c.open:
            if ui_button(c, 20, 20, 120, 34, "Click me"):
                print("clicked")
            ui_present(c)
    ui_fini(c)
    return 0
```

```powershell
cargo run -- run examples\ui_demo.ax -l user32 -l gdi32
```

Details: [`docs/ui.md`](docs/ui.md) · demo: `examples/ui_demo.ax` ·
tests: `tests/ui.rs`.

## Performance

Same-algorithm comparisons against `clang -O3` on the same machine (warm
runs, best of 3):

| Benchmark | Scale | Aoxn | clang C++ |
|---|---|---|---|
| Loop sum | 2×10⁸ iterations | ~15 ms | ~23 ms |
| Array fill + scan | 2×10⁸ reads | 86 ms | 99 ms |
| Struct copies (by value) | 7.5×10⁷ copies | 237 ms | 206 ms |

Native code is native code — Aoxn sits within noise of clang.

Web servers too: the [`web/`](web/README.md) suite ships an HTTP/1.1 server
written in Aoxn and benchmarks it against the pnpm + Node.js + Next.js stack
on identical routes — it matches plain Node.js throughput at ~1/50 the p50
latency and serves 26-54x more requests than Next.js, from a single 173 KB
binary with a 5 MB RSS. Numbers: [docs/web-benchmark.md](docs/web-benchmark.md).

## Self-hosting

The compiler is rewritten in Aoxn and has reached a **fixed point**: the
Aoxn-written driver compiles the whole self-hosting compiler
(~7k lines: lexer, parser, typecheck with monomorphization, loader,
LLVM-C codegen, driver) into a stage-2 binary whose IR **and emitted object
files are byte-identical** to the Rust compiler's for the same program. It
also compiles the real stdlib and the full `examples/` suite. See
[`docs/selfhost.md`](docs/selfhost.md) and the
[wiki's Self-Hosting page](wiki/Self-Hosting.md).

## Project layout

| Path | Contents |
|---|---|
| `src/` | the compiler: lexer → parser → typecheck → LLVM codegen → clang link |
| `src/llvm.rs` | hand-written LLVM-C FFI (no inkwell/llvm-sys) |
| `src/codegen_c.rs` | experimental C-emitting backend (`--backend c`, v0.27.1) |
| `src/ts/` | the TypeScript front end (TS-M1 W1 complete: S2b type layer + S3 modules) |
| `stdlib/stdlib.ax` | the standard library, written in Aoxn itself |
| `stdlib/ui.ax`, `stdlib/ui_win.ax` | the UI toolkit (portable half + Win32 backend) |
| `examples/*.ax` | demo programs (hello, fib, primes, stdlib_demo, ui_demo, …) |
| `selfhost/` | the compiler rewritten in Aoxn (fixed point reached) |
| `web/` | web benchmark suite: an HTTP server in Aoxn vs pnpm+Node.js+Next.js |
| `tests/` | 134 end-to-end tests: compile → run → verify output (pipeline 98 + TS 34 + UI 2) |
| `docs/spec.md` | full language specification |
| `wiki/` | bilingual (中文/English) wiki — start at [`wiki/Home.md`](wiki/Home.md) |

## Testing & CI

`cargo test` runs 134 end-to-end tests (every test compiles `.ax` to an
executable, runs it and asserts stdout + exit code), including a
cross-backend test that runs the same programs through `--backend llvm` and
`--backend c` and compares byte-for-byte. CI runs the suite on
windows-latest, ubuntu-latest, macos-13 and macos-14 on every push. See
[`wiki/Testing-and-CI.md`](wiki/Testing-and-CI.md).

## Status

**v0.28.0** · Windows Tier 1, Linux/macOS Tier 2 · 134/134 tests green ·
self-hosting fixed point (byte-identical IR + object files) · UI toolkit in
the stdlib · experimental C-emitting backend (`--backend c`) · TS-M1 W1
complete (S2b type layer + S3 modules; the bare `import "path"` form is
gone — `import * from "path"`).

See [`docs/spec.md`](docs/spec.md) for the complete language specification
and [`CHANGELOG.md`](CHANGELOG.md) for the release history.

## License

Apache-2.0 — see [`LICENSE`](LICENSE).

---
---

# 中文

**Aoxn** 是一门 AI 原生、静态类型、提前编译的编程语言：表面是 Python 式语法，
底下是 C++ 级别的原生性能——程序经 LLVM（O3）直接编译成机器码。

## 为什么选 Aoxn

- **Python 式语法** —— 缩进块、`def` / `elif` / `pass`、`#` 注释、
  `x = 5` 类型推断、`and` / `or` / `not`、`[0] * n`
- **原生速度** —— LLVM O3 后端，实测与 `clang -O3` 同级（基准见下）
- **泛型** —— `def sort[T, N](arr: [T; N])` 按调用点单态化，无装箱、无运行时开销
- **AI 原生工具链** —— 诊断可输出结构化 JSON（`--json`）、优化后 IR 可导出
  （`aoxn ir`），语义刻意保持严格与确定，AI 生成的代码可被机器验证
- **值语义** —— 数组、结构体、字符串按值复制，没有隐藏引用；数组索引不检查（C 风格）

## 快速上手

前置（Windows，Tier 1）：Rust（msvc 主机）、LLVM（C API）、clang、MSVC
Build Tools。`build.rs` 自动定位 LLVM（可用 `AOXN_LLVM_DIR` 覆盖）。Linux 与
macOS 是 Tier 2，CI 全绿（见 [`docs/platform-support.md`](docs/platform-support.md)）。

```powershell
cargo build
cargo run -- run examples\hello.ax
```

编译出独立的原生可执行文件：

```powershell
cargo run -- build examples\fib.ax -o fib.exe
.\fib.exe
```

更多：`cargo run -- ir examples\fib.ax` 导出优化后的 LLVM IR；`--json`
输出机器可读诊断；`--cpu native` 针对宿主 CPU（AVX2 等）极致提速——默认的
通用 CPU 保证编译产物跨机器可复现。自 v0.27.1 起还有实验性的 **C 发射后端**：
`--backend c` 改走"生成 C + 同一套 clang 工具链"而不是 LLVM IR（示例输出逐
字节一致、运行性能 ±10% 内、7k 行输入编译时间 +6%——数据见
[`docs/llvm-independence-report.md`](docs/llvm-independence-report.md)）；默认
后端仍是 LLVM。

编译速度优先时用优化级别：

```powershell
cargo run -- run examples\fib.ax --O1     # default<O1>：大输入编译约快 2 倍；
                                          # O3 仍是默认
cargo run -- build examples\fib.ax --O0   # 跳过 IR 管线 + fast-isel 后端
```

`aoxn run` 与 `aoxn build` 按"程序内容哈希 + 编译器 + 选项"缓存产物
（`target/cache`）：重复运行/构建未改动的源码完全跳过编译与链接（实测
`examples/hello.ax` 约 1.1s → 85ms）。`AOXN_NO_CACHE=1` 关闭，`AOXN_CACHE_DIR`
迁移缓存目录。

## 语言速览

（代码示例同上方英文区：`Particle` 结构体、值语义、字符串逐字节比较。）

类型严格、每个表达式都检查：`int`/`float` 永不隐式互转，条件必须是 `bool`，
数组索引不检查（C 风格），函数所有路径必须返回。严格是特性：保证简单到机器
可以推理。

## 标准库与 UI 工具箱

`stdlib/stdlib.ax` 用 Aoxn 自己写成：泛型 `sort` / `binary_search` / 聚合、
可增长 `Vec`、字节缓冲、文件 IO 与进程调用。

自 v0.27.0 起标准库带有 **Qt 风格的立即模式 UI 工具箱**——纯 Aoxn + 原始
Win32/GDI FFI：按钮、复选框、滑条、进度条、标签、面板、亮/暗配色、
UTF-8/emoji 文本、轮询输入。控件是每帧调用的普通函数，状态由应用自己持有
——这正是"暂无回调"的语言所能承载的形态（用法示例见上方英文区）：

```powershell
cargo run -- run examples\ui_demo.ax -l user32 -l gdi32
```

细节见 [`docs/ui.md`](docs/ui.md)；演示 `examples/ui_demo.ax`；测试
`tests/ui.rs`。

## 性能

同算法与 `clang -O3` 同机对比（预热后 3 次取最优）：

| 基准 | 规模 | Aoxn | clang C++ |
|---|---|---|---|
| 循环求和 | 2×10⁸ 次迭代 | ~15 ms | ~23 ms |
| 数组填充 + 扫描 | 2×10⁸ 次读 | 86 ms | 99 ms |
| 结构体复制（按值） | 7.5×10⁷ 次 | 237 ms | 206 ms |

原生代码就是原生代码——Aoxn 与 clang 在噪声范围内持平。

Web 服务同样能打：[`web/`](web/README.md) 套件用 Aoxn 写了 HTTP/1.1 服务器，
在相同路由上对阵 pnpm + Node.js + Next.js——吞吐打平纯 Node.js、p50 延迟约为
其 1/50，比 Next.js 多服务 26–54 倍请求，单个 173 KB 二进制、5 MB 内存。
数据见 [docs/web-benchmark.md](docs/web-benchmark.md)。

## 自举

编译器已用 Aoxn 重写并达到**固定点**：Aoxn 写的 driver 编译整个自举编译器
（约 7k 行：词法、语法、带单态化的类型检查、装载器、LLVM-C 代码生成、驱动）
得到 stage-2，产物的 IR 与目标文件和 Rust 编译器**逐字节一致**，并能编译真
stdlib 与全部 `examples/`。详见 [`docs/selfhost.md`](docs/selfhost.md) 与
[wiki 的自举页](wiki/Self-Hosting.md)。

## 项目布局

（表格同上方英文区：`src/` 编译器、`src/codegen_c.rs` 实验性 C 发射后端
（`--backend c`，v0.27.1）、`src/ts/` TS 前端（TS-M1 W1 完成）、`stdlib/` 标准库 +
UI、`selfhost/` 自举、`web/` Web 基准、`tests/` 134 个端到端测试、
`docs/spec.md` 语言规范、`wiki/` 双语 wiki。）

## 测试与 CI

`cargo test` 跑 134 个端到端测试（每个测试都是 .ax → 可执行文件 → 运行 →
断言 stdout 与退出码），其中含跨后端测试：同一程序分别走 `--backend llvm`
与 `--backend c` 编译运行、输出逐字节比对。每次 push 在 windows-latest、
ubuntu-latest、macos-13、macos-14 四平台跑全套件。详见
[`wiki/Testing-and-CI.md`](wiki/Testing-and-CI.md)。

## 现状

**v0.28.0** · Windows Tier 1，Linux/macOS Tier 2 · 134/134 测试全绿 ·
自举固定点（IR + 目标文件逐字节一致）· 标准库内置 UI 工具箱 · 实验性
C 发射后端（`--backend c`）· TS-M1 W1 收官（S2b 类型层 + S3 模块系统；
旧 `import "path"` 已删除——用 `import * from "path"`）。

完整语言规范见 [`docs/spec.md`](docs/spec.md)，发布历史见
[`CHANGELOG.md`](CHANGELOG.md)。

## 许可证

Apache-2.0 —— 见 [`LICENSE`](LICENSE)。
