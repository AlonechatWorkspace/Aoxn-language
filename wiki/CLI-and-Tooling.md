# 命令行与工具链 · CLI and Tooling

> **中文**：`aoxn` 的三个子命令、全部旗标、优化级别、内容哈希构建缓存、诊断输出格式（含 `--json`）、退出码，以及完整的环境变量表。
> **English**: The three `aoxn` subcommands, every flag, optimization levels, the content-hash build cache, diagnostic formats (including `--json`), exit codes, and the complete environment-variable table.

## 中文

### 1. 用法总览

```text
aoxn build <file.ax> [-o out] [--O0|--O1|--O2|--O3] [--json] [-l lib] [-L dir]
aoxn run   <file.ax> [--O0|--O1|--O2|--O3] [--json] [-l lib] [-L dir] [-- args...]
aoxn ir    <file.ax> [--O0|--O1|--O2|--O3] [--json]
aoxn --help
```

| 子命令 | 作用 |
|---|---|
| `build` | 编译成独立可执行文件，默认输出 `<输入文件名>.exe`（非 Windows 上无扩展名）；成功时把输出路径打印到 stdout |
| `run` | 编译并立即运行；编译出的可执行文件位于构建缓存中，进程的 stdout/stderr 直接继承，`aoxn` 的退出码就是程序退出码 |
| `ir` | 打印优化后的 LLVM IR 到 stdout（不写文件、不链接；此时 `-l` / `-L` 不参与） |

日常都用 `cargo run -- <子命令>`，或者先 `cargo build` 再直接用 `.\target\debug\aoxn.exe`（少了 cargo 的启动开销）。

### 2. 旗标

| 旗标 | 说明 |
|---|---|
| `-o <path>` | 输出可执行文件路径（仅 `build`；默认是输入文件名换扩展名） |
| `--O0` / `--O1` / `--O2` / `--O3` | 优化级别；**最多给一个**，给两个直接 `exit 2`；也接受单横线写法 `-O1` 等 |
| `--cpu <cpu>` | 传给 LLVM 目标机的 CPU 名（如 `skylake`、`x86-64`）；留空 = 通用的 `generic`，保证输出可复现。注意 **`native` 不被 LLVM 的 C API 解析**：本机实测 `--cpu native` 报 `'native' is not a recognized processor for this target` 并退出 1（clang 会在驱动层把 `native` 解析成真实 CPU 名，Aoxn 没有这一层）。C 后端下 `native` 合法（clang `-march=native`） |
| `--backend <llvm\|c>` | 代码生成后端（v0.27.1）：`llvm`（默认）= 现有 LLVM IR 管线；`c` = **实验性 C 发射后端**，生成 C99 文本交给同一套 clang 工具链编译（输出与 LLVM 后端逐字节一致，运行性能 ±10% 内、编译时间约 +6%，见 [LLVM 独立性调查](../docs/llvm-independence-report.md) §7）。C 后端下 `aoxn ir` 仍打 LLVM IR、`AOXN_PASSES` 不适用 |
| `--json` | 诊断以 JSON 输出到 **stderr** |
| `-l <name>` / `-L <dir>` | 额外链接库与库搜索路径，可重复，原样转发给 clang |
| `--` | 其后所有参数作为**被运行程序**的参数（仅 `run`） |
| 位置参数 | 输入 `.ax` 文件；可以给多个（它们会被合并，并对每个文件解析 `import`） |

参数是分开的 token：写 `-o out.exe`，不支持 `-o=out.exe` 或 `--out out.exe`。

```powershell
cargo run -- run examples\hello.ax                       # 最快的一次性用法
cargo run -- build examples\fib.ax -o fib.exe            # 产出独立可执行文件
cargo run -- ir examples\fib.ax                          # 看优化后的 IR
cargo run -- run examples\hello.ax --O1                  # 编译更快的档位
cargo run -- run web\server_win.ax -l ws2_32             # 链接 ws2_32
cargo run -- run myprog.ax -- --verbose file.txt         # 把参数传给程序
```

### 3. 优化级别

| 级别 | LLVM 管线 | 定位 |
|---|---|---|
| `--O3`（默认） | `default<O3>` | 运行时最快；"与 `clang -O3` 同性能"这一承诺对应的档位 |
| `--O2` | `default<O2>` | 实测编译时间与 O3 相同（换 O2 不提速），保留只为完整性 |
| `--O1` | `default<O1>` | 编译时间约减半（大输入实测 passes −49%），代价是内联密集的代码运行时变慢（fib −38%），循环型代码基本无损 |
| `--O0` | 跳过 IR 管线 + fast-isel 后端 | 编译最快、代码最慢，适合只关心"能不能编译"的场景 |

- `AOXN_PASSES=<pipeline>` 可以在任何 `> 0` 的级别上覆盖管线文本（例如 `AOXN_PASSES=default<O1>`）。
- `--O0` 与 `AOXN_PASSES` 之外，其余级别的 IR 构造完全相同，因此自举固定点（IR 与 COFF 逐字节一致）只在默认档受到保护。
- 细节与实测数据见 [性能与基准](Performance-and-Benchmarks.md)。

### 4. 构建缓存

`run` 与 `build` **共用**一个内容哈希缓存："改一行再跑"的开发循环因此从"每次都要编译 + 链接"变成命中即跳过。

- **缓存键**包含：入口文件与全部传递导入的**内容**、编译器可执行文件自身（大小 + mtime）、优化级别、`AOXN_CPU`、`AOXN_PASSES`、`AOXN_BACKEND`、`-l` 列表、`-L` 列表，以及解析到的 clang 路径。任何一项变化都是 miss。
- **位置**：`AOXN_CACHE_DIR` → 否则 `<当前目录>/target/cache`（不可写则退到系统临时目录）。条目名就是 16 位十六进制键 + 可执行文件后缀。
- **命中行为**：`run` 直接执行缓存里的可执行文件；`build` 把缓存文件**拷贝**到 `-o` 目标（缓存条目保留给下次使用）。命中时会更新条目 mtime 以便存活于清理。
- **容量**：最多 64 条，按 mtime 的近似 LRU 清理（未命中的并发临时文件 `<key>.<pid>` 不参与计数）。
- **并发安全**：先构建到 `<key>.<pid>` 临时文件，再用 rename 发布，避免两个并发调用写同一个输出。
- **关闭**：`AOXN_NO_CACHE=1`（只要该变量**存在**就生效，值是 `0` 也算设置）。

```powershell
$env:AOXN_NO_CACHE = "1"                 # 完全禁用缓存
$env:AOXN_CACHE_DIR = "D:\aoxn-cache"    # 换缓存目录
```

缓存只影响"是否重新编译"，不影响产物语义：miss 时全量编译的结果与命中时拷出来的字节一致。

### 5. 诊断输出

人类可读格式（stderr，每行一条）：

```text
[type] myprog.ax:1:1: function 'bad1' returns int but does not return a value on all paths
```

`stage` 取值：`lex` | `parse` | `type` | `internal` | `link` | `io`。`internal` 表示编译器自身的失败
（按项目纪律，内部失败必须变成诊断而不是 panic）。

JSON 格式（`--json`，同样走 stderr）：

```json
{"ok":false,"errors":[{"stage":"type","file":"myprog.ax","line":1,"col":1,"message":"function 'bad1' returns int but does not return a value on all paths"}]}
```

这是给工具/Agent 消费的稳定契约：位置是 1 基的行列，`file` 是触发诊断的文件（多文件时指向真正的来源文件，而不是入口文件）。

### 6. 退出码

| 退出码 | 含义 |
|---|---|
| 0 | 成功（`run` 时即被运行程序返回 0） |
| 1 | 编译失败（词法/语法/类型/内部/链接/IO 错误），或 `run` 无法启动产物 |
| 2 | 用法错误：没有参数（打印帮助）、未知子命令、同一次调用给了多个 `--O*`、缺少输入文件 |
| 其他 | `run` 会原样传递被运行程序的退出码 |

### 7. 环境变量

| 变量 | 作用 | 备注 |
|---|---|---|
| `AOXN_PASSES` | 覆盖 LLVM 管线文本 | 任何 `> 0` 的优化级别下生效，例如 `default<O1>` |
| `AOXN_CPU` | 目标 CPU | 等价于 `--cpu`；空值 = 默认 generic（保证可复现） |
| `AOXN_BACKEND` | 代码生成后端 | 等价于 `--backend`：`c` = 实验性 C 发射后端，`llvm`（或未设）= 默认 LLVM 后端；测试套件用它整体切后端 |
| `AOXN_NO_CACHE` | 禁用构建缓存 | 只要变量存在即生效 |
| `AOXN_CACHE_DIR` | 缓存目录 | 默认 `<cwd>/target/cache` |
| `AOXN_DUMP_IR` | 把 verify 之前的 IR 打到 stderr | 存在即生效 |
| `AOXN_TIME` | 打印各阶段墙钟时间 | `lex` / `parse` / `typecheck` / `cg.*` / `link` |
| `AOXN_TC_TRACE` | 类型检查逐函数标记 | 存在即生效，排查"卡在哪个函数" |
| `AOXN_CG_TRACE` | 代码生成逐函数标记 | 存在即生效 |
| `AOXN_LLVM_DIR` | LLVM 安装目录 | 主要在 `build.rs`（编译本项目）时用；`platform::llvm_link_name()` 也用它探测库名 |
| `AOXN_LLVM_LIB` | 强制 LLVM 链接名 | 覆盖按平台/目录探测的结果 |
| `AOXN_CLANG` | 指定 clang 可执行文件 | 优先于 `PATH` 与默认位置 |

`AOXN_DUMP_IR` / `AOXN_TIME` / `AOXN_TC_TRACE` / `AOXN_CG_TRACE` / `AOXN_NO_CACHE` 都是"存在即生效"：设成 `0` 也会打开，想关掉请删除变量。

```powershell
$env:AOXN_TIME = "1";   cargo run -- run examples\stdlib_demo.ax
$env:AOXN_DUMP_IR = "1"; cargo run -- ir examples\fib.ax
```

### 8. 多文件与链接

- `aoxn build a.ax b.ax` 会把多个入口文件合并（每个文件的 `import` 各自解析），除入口外的文件只提供名字。
- 需要 C 库时用 `-l` / `-L`（`-l ws2_32`、`-l LLVM-C -L "C:\Program Files\LLVM\lib"`）。
- 最终链接由 clang 完成，顺序是 `AOXN_CLANG` → `PATH` → 仓库内 `LLVM\bin\clang.exe` → `C:\Program Files\LLVM\bin\clang.exe`；平台差异（栈大小标志、rpath、扩展名）由 `src/platform.rs` 处理，见 [平台支持](Platform-Support.md)。

## English

### 1. Usage at a glance

```text
aoxn build <file.ax> [-o out] [--O0|--O1|--O2|--O3] [--json] [-l lib] [-L dir]
aoxn run   <file.ax> [--O0|--O1|--O2|--O3] [--json] [-l lib] [-L dir] [-- args...]
aoxn ir    <file.ax> [--O0|--O1|--O2|--O3] [--json]
aoxn --help
```

| Subcommand | Purpose |
|---|---|
| `build` | compile to a standalone executable, default output `<input>.exe` (no extension off Windows); prints the output path to stdout on success |
| `run` | compile and run immediately; the executable lives in the build cache, the child inherits stdout/stderr, and `aoxn` exits with the program's exit code |
| `ir` | print the optimized LLVM IR to stdout (no object, no link; `-l` / `-L` are ignored here) |

Day to day, use `cargo run -- <subcommand>`, or `cargo build` once and then call
`.\target\debug\aoxn.exe` directly to skip cargo's startup cost.

### 2. Flags

| Flag | Meaning |
|---|---|
| `-o <path>` | output executable path (`build` only; defaults to the input name with the platform extension) |
| `--O0` / `--O1` / `--O2` / `--O3` | optimization level; **at most one** (`exit 2` otherwise); single-dash forms (`-O1`) are accepted too |
| `--cpu <cpu>` | CPU name handed to the LLVM target machine (e.g. `skylake`, `x86-64`); empty = the generic CPU, which keeps output reproducible. Note that **`native` is not resolved by the LLVM C API**: measured on this machine, `--cpu native` reports `'native' is not a recognized processor for this target` and exits 1 (clang resolves `native` in its driver; Aoxn has no such layer). Under the C backend `native` is accepted (clang `-march=native`) |
| `--backend <llvm\|c>` | codegen backend (v0.27.1): `llvm` (default) = the existing LLVM IR pipeline; `c` = the **experimental C-emitting backend**, which generates C99 text and hands it to the same clang toolchain (byte-identical output, runtime within ±10%, ~+6% compile time — see [the LLVM-independence investigation](../docs/llvm-independence-report.md) §7). Under the C backend `aoxn ir` still prints LLVM IR and `AOXN_PASSES` does not apply |
| `--json` | emit diagnostics as JSON on **stderr** |
| `-l <name>` / `-L <dir>` | extra link libraries and search paths, repeatable, forwarded verbatim to clang |
| `--` | everything after it is passed to the **compiled program** (`run` only) |
| positional | input `.ax` files; several are allowed (they are merged, and `import`s are resolved for each) |

Flags are separate tokens: write `-o out.exe`, not `-o=out.exe` or `--out out.exe`.

```powershell
cargo run -- run examples\hello.ax                       # quickest one-shot use
cargo run -- build examples\fib.ax -o fib.exe            # standalone executable
cargo run -- ir examples\fib.ax                          # optimized IR
cargo run -- run examples\hello.ax --O1                  # faster-compiling level
cargo run -- run web\server_win.ax -l ws2_32             # link ws2_32
cargo run -- run myprog.ax -- --verbose file.txt         # pass arguments to the program
```

### 3. Optimization levels

| Level | LLVM pipeline | Positioning |
|---|---|---|
| `--O3` (default) | `default<O3>` | fastest runtime; the level behind the "parity with `clang -O3`" promise |
| `--O2` | `default<O2>` | measured to compile in the same time as O3 (switching to O2 buys nothing); kept for completeness |
| `--O1` | `default<O1>` | about half the compile time (passes −49% on large inputs), at the cost of slower inlining-heavy code (fib −38%); loop-heavy code is essentially unaffected |
| `--O0` | no IR pipeline + fast-isel backend | fastest compile, slowest code; for "does it compile at all" loops |

- `AOXN_PASSES=<pipeline>` overrides the pipeline text at any level `> 0` (for
  example `AOXN_PASSES=default<O1>`).
- Apart from `--O0` and `AOXN_PASSES`, the IR construction is identical across
  levels, which is why the self-hosting fixed point (byte-identical IR and COFF
  objects) is only protected at the default level.
- Numbers and method: [Performance and Benchmarks](Performance-and-Benchmarks.md).

### 4. Build cache

`run` and `build` **share** one content-hash cache, turning the "edit a line, run
again" loop from compile + link every time into a cache hit.

- **Key contents**: the **content** of the entry file and all transitive imports,
  the compiler executable itself (size + mtime), the optimization level,
  `AOXN_CPU`, `AOXN_PASSES`, `AOXN_BACKEND`, the `-l` list, the `-L` list, and
  the resolved clang path. Any change is a miss.
- **Location**: `AOXN_CACHE_DIR`, else `<cwd>/target/cache` (falling back to the
  system temp dir when that is not writable). Entries are named after the 16-hex
  key plus the executable suffix.
- **On a hit**: `run` executes the cached executable; `build` **copies** it to the
  `-o` destination (the cache entry survives). Hits bump the entry's mtime so it
  survives pruning.
- **Bound**: 64 entries, pruned oldest-first (approximate LRU); concurrent
  `<key>.<pid>` temp files are not counted.
- **Concurrency**: builds land in a `<key>.<pid>` temp file and are published
  with a rename, so two concurrent invocations never race on one output.
- **Disable**: `AOXN_NO_CACHE=1` (the variable only has to **exist**; `0` counts
  as set).

```powershell
$env:AOXN_NO_CACHE = "1"                 # disable the cache entirely
$env:AOXN_CACHE_DIR = "D:\aoxn-cache"    # relocate it
```

The cache only decides whether to recompile; it never changes the product — a
miss compiles to the same bytes a hit copies out.

### 5. Diagnostics

Human-readable form (stderr, one line per diagnostic):

```text
[type] myprog.ax:1:1: function 'bad1' returns int but does not return a value on all paths
```

`stage` is one of `lex` | `parse` | `type` | `internal` | `link` | `io`.
`internal` means the compiler itself failed — by project policy an internal
failure must surface as a diagnostic, never a panic.

JSON form (`--json`, also on stderr):

```json
{"ok":false,"errors":[{"stage":"type","file":"myprog.ax","line":1,"col":1,"message":"function 'bad1' returns int but does not return a value on all paths"}]}
```

This is the stable contract for tools and agents: 1-based line/column, and `file`
names the file that actually produced the diagnostic (with several files it
points at the real source, not the entry file).

### 6. Exit codes

| Code | Meaning |
|---|---|
| 0 | success (for `run`, the program returned 0) |
| 1 | compilation failed (lex/parse/type/internal/link/io), or `run` could not start the product |
| 2 | usage error: no arguments (prints help), unknown subcommand, several `--O*` flags in one invocation, missing input file |
| other | `run` forwards the compiled program's exit code verbatim |

### 7. Environment variables

| Variable | Effect | Notes |
|---|---|---|
| `AOXN_PASSES` | override the LLVM pipeline text | applies at any level `> 0`, e.g. `default<O1>` |
| `AOXN_CPU` | target CPU | same as `--cpu`; empty = default generic (keeps output reproducible) |
| `AOXN_BACKEND` | codegen backend | same as `--backend`: `c` = experimental C-emitting backend, `llvm` (or unset) = the default LLVM backend; the test suite uses it to run everything through one backend |
| `AOXN_NO_CACHE` | disable the build cache | presence is enough |
| `AOXN_CACHE_DIR` | cache location | default `<cwd>/target/cache` |
| `AOXN_DUMP_IR` | dump pre-verify IR to stderr | presence is enough |
| `AOXN_TIME` | per-stage wall clock | `lex` / `parse` / `typecheck` / `cg.*` / `link` |
| `AOXN_TC_TRACE` | per-function typecheck markers | presence is enough; finds "which function is stuck" |
| `AOXN_CG_TRACE` | per-function codegen markers | presence is enough |
| `AOXN_LLVM_DIR` | LLVM install directory | mainly for `build.rs` (building this project); `platform::llvm_link_name()` probes library names there too |
| `AOXN_LLVM_LIB` | force the LLVM link name | overrides the platform/directory probe |
| `AOXN_CLANG` | clang executable | takes priority over `PATH` and the default locations |

`AOXN_DUMP_IR` / `AOXN_TIME` / `AOXN_TC_TRACE` / `AOXN_CG_TRACE` / `AOXN_NO_CACHE`
are presence-sensitive: setting them to `0` still turns them on — delete the
variable to turn them off.

```powershell
$env:AOXN_TIME = "1";    cargo run -- run examples\stdlib_demo.ax
$env:AOXN_DUMP_IR = "1"; cargo run -- ir examples\fib.ax
```

### 8. Multiple files and linking

- `aoxn build a.ax b.ax` merges several entry files (each one's `import`s are
  resolved); the non-entry files just contribute names.
- Extra C libraries go through `-l` / `-L` (`-l ws2_32`,
  `-l LLVM-C -L "C:\Program Files\LLVM\lib"`).
- The final link is performed by clang, found in this order: `AOXN_CLANG` →
  `PATH` → repo-local `LLVM\bin\clang.exe` → `C:\Program Files\LLVM\bin\clang.exe`.
  Platform differences (stack-size flag, rpath, executable extension) are handled
  by `src/platform.rs` — see [Platform Support](Platform-Support.md).

---

## 源文件 / Source files

- [src/main.rs](../src/main.rs) — CLI parsing, exit codes, cache key, pruning, publishing
- [src/lib.rs](../src/lib.rs) — `diag_to_string` / `diags_to_json`, clang lookup, `AOXN_TIME`
- [src/codegen.rs](../src/codegen.rs) — optimization pipeline selection, `AOXN_DUMP_IR`, `AOXN_CG_TRACE`
- [src/typecheck.rs](../src/typecheck.rs) — `AOXN_TC_TRACE`
- [src/platform.rs](../src/platform.rs) — `AOXN_LLVM_LIB`, executable extension
- [CONTRIBUTING.md](../CONTRIBUTING.md) — the debug-switch table this page expands
- [docs/optimization-report.md](../docs/optimization-report.md) — measured numbers behind the cache and the levels
