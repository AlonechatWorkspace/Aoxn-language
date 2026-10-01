# Aoxn

**Aoxn** is an AI-native, statically typed, ahead-of-time compiled programming
language. Python-style syntax on the surface, C++-class native performance
underneath: programs lower to ISO C and compile with clang (O3) straight to
machine code — the compiler itself carries no LLVM dependency.

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
- **Native speed** — generated C compiled by `clang -O3`; measured at parity
  with hand-written C (benchmarks below)
- **Generics** — `def sort[T, N](arr: [T; N])` monomorphized per call site;
  no boxing, no runtime overhead
- **AI-native tooling** — every diagnostic can be emitted as structured JSON
  (`--json`), the generated C is dumpable (`aoxn c`), and the semantics are
  deliberately strict and deterministic so AI-generated code is verifiable
- **Value semantics** — arrays, structs, and strings copy by value; no hidden
  references; array indexing is unchecked, C-style

## Quick start

Prerequisites (Windows, Tier 1): Rust (msvc host), clang, MSVC Build Tools.
Since v0.29.0 the compiler carries no LLVM dependency — `cargo build` needs
only the Rust toolchain, and compiling programs needs only clang. Linux and
macOS are Tier 2 and fully green in CI (see
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

More: `cargo run -- c examples\fib.ax` prints the generated C text (the
compiler's only backend since v0.29.0 — the LLVM backend was removed, see
[`docs/llvm-independence-report.md`](docs/llvm-independence-report.md));
`cargo run -- run examples\primes.ax --json` emits machine-readable
diagnostics; `--cpu native` targets the host CPU (AVX2 & co.) for maximum
speed — the default generic CPU keeps compiled output reproducible across
machines.

Optimization levels, for when compile time matters more than runtime speed
(the level selects the clang `-O` used to compile the generated C):

```powershell
cargo run -- run examples\fib.ax --O1     # clang -O1: faster compile,
                                          # O3 stays the default
cargo run -- build examples\fib.ax --O0   # clang -O0
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
C-text codegen, driver) into a stage-2 binary whose generated C **and emitted
object files are byte-identical** to the Rust-built compiler's for the same
program. It also compiles the real stdlib and the full `examples/` suite. See
[`docs/selfhost.md`](docs/selfhost.md) and the
[wiki's Self-Hosting page](wiki/Self-Hosting.md).

## Project layout

| Path | Contents |
|---|---|
| `src/` | the compiler: lexer → parser → typecheck → C-text codegen → clang link |
| `src/codegen_c.rs` | the C-emitting backend (the only backend since v0.29.0) |
| `src/ts/` | the TypeScript front end (TS-M1 W1 complete: S2b type layer + S3 modules) |
| `stdlib/stdlib.ax` | the standard library, written in Aoxn itself |
| `stdlib/ui.ax`, `stdlib/ui_win.ax` | the UI toolkit (portable half + Win32 backend) |
| `examples/*.ax` | demo programs (hello, fib, primes, stdlib_demo, ui_demo, …) |
| `selfhost/` | the compiler rewritten in Aoxn (fixed point reached) |
| `web/` | web benchmark suite: an HTTP server in Aoxn vs pnpm+Node.js+Next.js |
| `tests/` | end-to-end tests: compile → run → verify output, including the self-hosting fixed point (byte-identical generated C + objects) |
| `docs/spec.md` | full language specification |
| `wiki/` | bilingual (中文/English) wiki — start at [`wiki/Home.md`](wiki/Home.md) |

## Testing & CI

`cargo test` runs the end-to-end suite (every test compiles `.ax` to an
executable, runs it and asserts stdout + exit code), including the
self-hosting fixed point: the stage-1 and stage-2 compilers must emit
byte-identical C and object files for the same program (the object comparison
masks the COFF TimeDateStamp that clang stamps into every Windows object).
CI runs the suite on
windows-latest, ubuntu-latest and macos-14 on every push. See
[`wiki/Testing-and-CI.md`](wiki/Testing-and-CI.md).

## Package management

`aoxn pkg` (and the direct aliases `aoxn init | add | remove | install |
update | outdated | tree | why | publish | yank | audit | cache`) manages
dependencies through `aoxn.json` + `aoxn.lock` into `aox_modules/`, resolving
via PubGrub against directory or git registries (beta; see
[`crates/aoxn-pkg`](crates/aoxn-pkg)). Since v0.29.1 a bare package import
resolves its entry through the package's `aoxn.json` — `main`, `exports`
(incl. `pkg/sub` subpaths), and `types` — so an installed package whose
entry is not `index.ax` is importable:

```Aoxn
import * from "http"          # → aox_modules/http/aoxn.json `main`/`exports["."]`
import * from "http/client"   # → exports["./client"]
```

A package without a manifest falls back to the legacy `aox_modules/<name>`
directory probe (`<name>.ax` / `index.ax`).

## Status

**v0.29.1** · Windows Tier 1, Linux/macOS Tier 2 · 173 tests green
(pipeline 99 + lib 6 + TS 34 + UI 2 + aoxn-pkg 32) · self-hosting fixed
point (byte-identical generated C + object files) · UI toolkit in the
stdlib · no LLVM dependency: the C-emitting backend is the only backend
(clang compiles it) · TS-M1 W1 complete (S2b type layer + S3 modules; the
bare `import "path"` form is gone — `import * from "path"`) · package
manager W2 step 1: manifest entry resolution (`main` / `exports` / `types`)
on the compiler side.

See [`docs/spec.md`](docs/spec.md) for the complete language specification
and [`CHANGELOG.md`](CHANGELOG.md) for the release history.

## License

Apache-2.0 — see [`LICENSE`](LICENSE).

---
---

# 中文

**Aoxn** 是一门 AI 原生、静态类型、提前编译的编程语言：表面是 Python 式语法，
底下是 C++ 级别的原生性能——程序降级为 ISO C 后由 clang（O3）直接编译成机器码，
编译器自身不携带任何 LLVM 依赖。

## 为什么选 Aoxn

- **Python 式语法** —— 缩进块、`def` / `elif` / `pass`、`#` 注释、
  `x = 5` 类型推断、`and` / `or` / `not`、`[0] * n`
- **原生速度** —— 生成的 C 交给 `clang -O3` 编译，实测与手写 C 同级（基准见下）
- **泛型** —— `def sort[T, N](arr: [T; N])` 按调用点单态化，无装箱、无运行时开销
- **AI 原生工具链** —— 诊断可输出结构化 JSON（`--json`）、生成的 C 文本可
  导出（`aoxn c`），语义刻意保持严格与确定，AI 生成的代码可被机器验证
- **值语义** —— 数组、结构体、字符串按值复制，没有隐藏引用；数组索引不检查（C 风格）

## 快速上手

前置（Windows，Tier 1）：Rust（msvc 主机）、clang、MSVC Build Tools。自
v0.29.0 起编译器不再依赖 LLVM——`cargo build` 只需要 Rust 工具链，编译程序只
需要 clang。Linux 与 macOS 是 Tier 2，CI 全绿（见
[`docs/platform-support.md`](docs/platform-support.md)）。

```powershell
cargo build
cargo run -- run examples\hello.ax
```

编译出独立的原生可执行文件：

```powershell
cargo run -- build examples\fib.ax -o fib.exe
.\fib.exe
```

更多：`cargo run -- c examples\fib.ax` 打印生成的 C 文本（v0.29.0 起这是唯一
后端——LLVM 后端已移除，见
[`docs/llvm-independence-report.md`](docs/llvm-independence-report.md)）；
`--json` 输出机器可读诊断；`--cpu native` 针对宿主 CPU（AVX2 等）极致提速——
默认的通用 CPU 保证编译产物跨机器可复现。

编译速度优先时用优化级别（级别选择编译生成 C 所用的 clang `-O`）：

```powershell
cargo run -- run examples\fib.ax --O1     # clang -O1：编译更快；默认仍是 O3
cargo run -- build examples\fib.ax --O0   # clang -O0
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
（约 7k 行：词法、语法、带单态化的类型检查、装载器、C 文本代码生成、驱动）
得到 stage-2，产物（生成的 C 与目标文件）和 Rust 侧构建的编译器**逐字节一致**，
并能编译真 stdlib 与全部 `examples/`。详见 [`docs/selfhost.md`](docs/selfhost.md)
与 [wiki 的自举页](wiki/Self-Hosting.md)。

## 项目布局

（表格同上方英文区：`src/` 编译器、`src/codegen_c.rs` 唯一的 C 发射后端
（v0.29.0 起）、`src/ts/` TS 前端（TS-M1 W1 完成）、`stdlib/` 标准库 +
UI、`selfhost/` 自举、`web/` Web 基准、`tests/` 端到端测试（含 C 文本
固定点）、`docs/spec.md` 语言规范、`wiki/` 双语 wiki。）

## 测试与 CI

`cargo test` 跑端到端测试套件（每个测试都是 .ax → 可执行文件 → 运行 →
断言 stdout 与退出码），其中含自举固定点：stage-1 与 stage-2 编译器对同一
程序必须产出逐字节一致的 C 文本与目标文件（目标文件比较会屏蔽 clang 写入
每个 Windows 目标文件的 COFF 时间戳）。每次 push 在 windows-latest、
ubuntu-latest、macos-14 三平台跑全套件。详见
[`wiki/Testing-and-CI.md`](wiki/Testing-and-CI.md)。

## 包管理

`aoxn pkg`（及直接别名 `aoxn init | add | remove | install | update |
outdated | tree | why | publish | yank | audit | cache`）通过 `aoxn.json` +
`aoxn.lock` 把依赖装进 `aox_modules/`，用 PubGrub 对目录或 git registry 做
解析（beta；见 [`crates/aoxn-pkg`](crates/aoxn-pkg)）。自 v0.29.1 起，裸包
导入按包内 `aoxn.json` 的 `main` / `exports`（含 `pkg/sub` 子路径）/ `types`
解析入口，装进来的包即便入口不叫 `index.ax` 也能 import：

```Aoxn
import * from "http"          # → aox_modules/http/aoxn.json 的 main/exports["."]
import * from "http/client"   # → exports["./client"]
```

没有 manifest 的包回退到旧的 `aox_modules/<name>` 目录探针（`<name>.ax` /
`index.ax`）。

## 现状

**v0.29.1** · Windows Tier 1，Linux/macOS Tier 2 · 173 测试全绿
（pipeline 99 + lib 6 + TS 34 + UI 2 + aoxn-pkg 32）· 自举固定点
（生成的 C + 目标文件逐字节一致）· 标准库内置 UI 工具箱 · 零 LLVM 依赖：
C 发射后端是唯一后端（clang 编译生成物）· TS-M1 W1 收官（S2b 类型层 +
S3 模块系统；旧 `import "path"` 已删除——用 `import * from "path"`）·
包管理器 W2 第一步：编译器侧 manifest 入口解析（`main` / `exports` /
`types`）。

完整语言规范见 [`docs/spec.md`](docs/spec.md)，发布历史见
[`CHANGELOG.md`](CHANGELOG.md)。

## 许可证

Apache-2.0 —— 见 [`LICENSE`](LICENSE)。
