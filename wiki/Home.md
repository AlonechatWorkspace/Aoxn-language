# Aoxn 编译器 Wiki · The Aoxn Compiler Wiki

> **中文**：Aoxn 是一门 AI 原生、静态类型、AOT 编译的编程语言——Python 风格语法，经 LLVM 编译为原生机器码；本 Wiki 是这门语言与这个编译器的完整文档入口。
> **English**: Aoxn is an AI-native, statically typed, ahead-of-time compiled language — Python-style syntax, compiled to native machine code through LLVM; this wiki is the documentation hub for both the language and the compiler.

## 中文

### 这是什么

这个仓库**本身就是编译器**：Rust 编写、**零外部 crate 依赖**、手写 LLVM-C FFI，把 `.ax` 源文件编译成原生可执行文件。语言的设计目标是最小语法、明确语义、原生速度（承诺与 `clang -O3` 同性能），并且对机器友好（`--json` 诊断、可 dump 的 IR、严格到可以被自动验证的语义）。

一分钟示例：

```aoxn
struct Particle:
    x: float
    mass: float

def energy(p: Particle) -> float:
    return p.x * p.x * p.mass

def main() -> int:
    ps: [Particle; 3] = [Particle(x=1.5, mass=2.0)] * 3
    total = 0.0
    for p in ps:
        total = total + energy(p)
    print(f"total = {total}")
    return 0
```

### 事实速览（v0.27.1）

| 项目 | 现状 |
|---|---|
| 语言 / 扩展名 | Aoxn（原名 Axon）/ `.ax` |
| 语法风格 | Python 式缩进块、`def` / `elif` / `pass`、`#` 注释、`and` / `or` / `not` |
| 编译目标 | LLVM `default<O3>` → 目标文件 → 由 clang 链接成可执行文件；v0.27.1 起另有实验性 C 发射后端（`--backend c`：生成 C → 同一套 clang 工具链，输出与 LLVM 后端逐字节一致） |
| 编译器 | Rust，零外部 crate；手写 LLVM-C FFI（不用 inkwell / llvm-sys） |
| 测试 | `cargo test` 128 个端到端测试（pipeline 98 + TS 前端 28 + UI 2）：编译 → 运行 → 断言 stdout 与退出码 |
| 平台 | Tier 1：Windows x86_64；Tier 2：Linux x86_64、macOS x86_64、macOS arm64（CI 四平台矩阵） |
| 自举 | 编译器已用 Aoxn 重写并跑通固定点：Rust 编译器与 Aoxn 编译器对同一程序产出的 IR 与 COFF 目标文件逐字节一致 |
| 标准库 | `stdlib/stdlib.ax` + UI 工具箱 `stdlib/ui.ax` / `stdlib/ui_win.ax`（v0.27.0，立即模式 GUI），全部用 Aoxn 自己写 |
| 工具链 | `aoxn build` / `aoxn run` / `aoxn ir`；`--O0`…`--O3`、`--backend llvm|c`、`--json`、内容哈希构建缓存 |

### 从这里开始

**使用 Aoxn 写程序**

- [快速上手](Getting-Started.md) —— 环境准备、构建编译器、第一个程序
- [语言速览](Language-Tour.md) —— 一次读完的语法tour
- [语言参考](Language-Reference.md) —— 完整规则：类型、值语义、泛型、导入、内建函数
- [标准库](Standard-Library.md) —— 泛型算法、`Vec`、字节缓冲、文件 IO、进程
- [命令行与工具链](CLI-and-Tooling.md) —— 子命令、优化级别、构建缓存、环境变量
- [常见问题与排错](Troubleshooting-FAQ.md)

**编译器内部原理**

- [编译器架构](Compiler-Architecture.md) —— 端到端流水线、诊断模型、多文件加载
- [词法与语法](Frontend-Lexer-and-Parser.md) —— 布局 token、AST、f-string 脱糖
- [类型检查](Type-Checker.md) —— 严格规则与泛型单态化
- [代码生成与 LLVM](Codegen-and-LLVM-FFI.md) —— 聚合 ABI 与全部 codegen 不变量
- [自举](Self-Hosting.md) —— 用 Aoxn 重写编译器，以及固定点验证

**工程与流程**

- [开发指南](Development-Guide.md) —— 改语言 / 改编译器的工作流与纪律
- [测试与 CI](Testing-and-CI.md) —— 128 个测试与四平台矩阵
- [平台支持](Platform-Support.md) —— Tier、LLVM 定位、链接名探测、遗留边界
- [性能与基准](Performance-and-Benchmarks.md) —— 编译耗时结构、优化级别取舍、测量方法
- [Web 平台](Web-Platform.md) —— 用 Aoxn 写的 HTTP 服务器与它的基准套件
- [路线图](Roadmap.md) · [术语表](Glossary.md)

### 文档现状说明

本 Wiki 以 v0.27.1 的**源码与测试**为准。仓库里仍有旧文档落后，阅读时请注意：

- `docs/spec.md` 标称 v0.9，其中 Statements 段写"`while` 是唯一的循环（还没有 `for`）"，而实现早已支持 `for`；
- `docs/selfhost.md` 是 lexer / parser / typecheck 时期、codegen 尚在早期的可行性评估稿（最后一次更新在 v0.19 前后），其中的进度与"剩余工作"已过时。

这些文档作为**深度报告**仍然有价值（平台调查、优化报告、web 基准、`docs/spec.md` 的语义细则），本 Wiki 在对应页面直接引用它们，而不再重复其原文。

### 如何发布到 GitHub Wiki

`wiki/` 目录是扁平的 GitHub Wiki 页面集合：`Home.md` 是首页，`_Sidebar.md` 与 `_Footer.md` 是侧栏与页脚，其余为普通页面。发布方式：

```powershell
git clone https://github.com/Ryan-178/Aoxn-language.wiki.git
Copy-Item wiki\*.md Aoxn-language.wiki\ -Force
cd Aoxn-language.wiki
git add -A
git commit -m "docs: publish wiki"
git push
```

页内互链统一写成 `Page-Name.md` 相对链接，在仓库内浏览与在 GitHub Wiki 上都能正常跳转。

### 贡献

改语言、改编译器、加测试的规则见 [开发指南](Development-Guide.md) 与仓库根的 `CONTRIBUTING.md`；语言规范的权威文本是 `docs/spec.md`，版本历史的权威来源是 `CHANGELOG.md`。

## English

### What this is

This repository **is** the compiler: written in Rust with **zero external crate
dependencies**, hand-written LLVM-C FFI, turning `.ax` sources into native
executables. The language aims at minimal syntax, explicit semantics, native
speed (the documented promise is parity with `clang -O3`), and machine-friendly
tooling (`--json` diagnostics, dumpable IR, semantics strict enough to be
verified automatically).

A one-minute example:

```aoxn
struct Particle:
    x: float
    mass: float

def energy(p: Particle) -> float:
    return p.x * p.x * p.mass

def main() -> int:
    ps: [Particle; 3] = [Particle(x=1.5, mass=2.0)] * 3
    total = 0.0
    for p in ps:
        total = total + energy(p)
    print(f"total = {total}")
    return 0
```

### Facts at a glance (v0.27.1)

| Item | Status |
|---|---|
| Language / extension | Aoxn (formerly Axon) / `.ax` |
| Syntax style | Python-style indentation blocks, `def` / `elif` / `pass`, `#` comments, `and` / `or` / `not` |
| Compilation target | LLVM `default<O3>` → object file → linked into an executable by clang; since v0.27.1 there is also an experimental C-emitting backend (`--backend c`: generated C → the same clang toolchain, byte-identical output) |
| Compiler | Rust, zero external crates; hand-written LLVM-C FFI (no inkwell / llvm-sys) |
| Tests | 128 end-to-end tests via `cargo test` (pipeline 98 + TS front end 28 + UI 2): compile → run → assert stdout and exit code |
| Platforms | Tier 1: Windows x86_64; Tier 2: Linux x86_64, macOS x86_64, macOS arm64 (four-platform CI matrix) |
| Self-hosting | The compiler has been rewritten in Aoxn and reaches the fixed point: the Rust and Aoxn compilers emit byte-identical IR and COFF objects for the same program |
| Standard library | `stdlib/stdlib.ax` + the UI toolkit `stdlib/ui.ax` / `stdlib/ui_win.ax` (v0.27.0, immediate-mode GUI), all written in Aoxn itself |
| Tooling | `aoxn build` / `aoxn run` / `aoxn ir`; `--O0`…`--O3`, `--backend llvm|c`, `--json`, content-hash build cache |

### Start here

**Using Aoxn**

- [Getting Started](Getting-Started.md) — prerequisites, building the compiler, first program
- [Language Tour](Language-Tour.md) — the whole syntax in one sitting
- [Language Reference](Language-Reference.md) — complete rules: types, value semantics, generics, imports, builtins
- [Standard Library](Standard-Library.md) — generic algorithms, `Vec`, byte buffers, file IO, processes
- [CLI and Tooling](CLI-and-Tooling.md) — subcommands, optimization levels, build cache, environment variables
- [Troubleshooting and FAQ](Troubleshooting-FAQ.md)

**Compiler internals**

- [Compiler Architecture](Compiler-Architecture.md) — end-to-end pipeline, diagnostics, multi-file loading
- [Lexer and Parser](Frontend-Lexer-and-Parser.md) — layout tokens, AST, f-string desugaring
- [Type Checker](Type-Checker.md) — strict rules and generic monomorphization
- [Codegen and LLVM](Codegen-and-LLVM-FFI.md) — aggregate ABI and every codegen invariant
- [Self-Hosting](Self-Hosting.md) — the compiler rewritten in Aoxn and the fixed-point verification

**Engineering and process**

- [Development Guide](Development-Guide.md) — workflow and discipline for changing the language or the compiler
- [Testing and CI](Testing-and-CI.md) — the 128 tests and the four-platform matrix
- [Platform Support](Platform-Support.md) — tiers, LLVM discovery, link-name probing, remaining gaps
- [Performance and Benchmarks](Performance-and-Benchmarks.md) — where compile time goes, optimization-level tradeoffs, measurement discipline
- [Web Platform](Web-Platform.md) — the HTTP server written in Aoxn and its benchmark suite
- [Roadmap](Roadmap.md) · [Glossary](Glossary.md)

### A note on stale docs

This wiki follows the **source and tests** of v0.27.1. A few documents in the
repository have fallen behind:

- `docs/spec.md` is labelled v0.9 and its Statements section still says `while`
  is the only loop ("no `for` yet"), although `for` has long been implemented.
- `docs/selfhost.md` is a feasibility assessment from the lexer/parser/typecheck
  era, written while codegen was still early (last updated around v0.19); its
  progress notes and "remaining work" list are out of date.

Those files remain valuable as **deep reports** (platform survey, optimization
report, web benchmark, and the semantic details in `docs/spec.md`); each wiki
page links to them instead of duplicating their text.

### Publishing to the GitHub Wiki

`wiki/` is a flat collection of GitHub Wiki pages: `Home.md` is the home page,
`_Sidebar.md` and `_Footer.md` are the sidebar and footer, everything else is a
regular page. To publish:

```powershell
git clone https://github.com/Ryan-178/Aoxn-language.wiki.git
Copy-Item wiki\*.md Aoxn-language.wiki\ -Force
cd Aoxn-language.wiki
git add -A
git commit -m "docs: publish wiki"
git push
```

In-page links are written as relative `Page-Name.md` links so they work both
when browsing the repository and on the GitHub Wiki.

### Contributing

The rules for changing the language, the compiler, and the tests live in the
[Development Guide](Development-Guide.md) and in `CONTRIBUTING.md` at the
repository root. `docs/spec.md` is the authoritative language specification and
`CHANGELOG.md` is the authoritative version history.

---

## 源文件 / Source files

- [Cargo.toml](../Cargo.toml) — version 0.27.0
- [tests/pipeline.rs](../tests/pipeline.rs) — the 97-test end-to-end suite (127 tests total with the TS + UI suites)
- [stdlib/stdlib.ax](../stdlib/stdlib.ax) — the standard library
- [src/main.rs](../src/main.rs) — the `aoxn` CLI
- [CONTRIBUTING.md](../CONTRIBUTING.md) — contribution rules
- [CHANGELOG.md](../CHANGELOG.md) — version history
