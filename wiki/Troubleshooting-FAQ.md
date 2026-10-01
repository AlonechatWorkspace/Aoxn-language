# 常见问题与排错 · Troubleshooting and FAQ

> **中文**：按"报错文本 → 原因 → 怎么改"整理的排错手册，外加使用 Aoxn 时最常见的语义误解与快问快答。
> **English**: A troubleshooting handbook organized as "message → cause → fix", plus the most common semantic misunderstandings and a quick FAQ.

## 中文

### 1. 环境与构建

| 症状 | 原因 | 处理 |
|---|---|---|
| `cargo run -- run ...` 报 "cannot find clang" | C 后端要用 clang 编译生成的 C 并链接 | 设 `AOXN_CLANG` 指向 clang 可执行文件，或把 clang 的 `bin` 目录加进 `PATH`（v0.29.0 起不再有任何 LLVM 要求） |
| 生成的 C 编译失败 | 代码生成器缺陷；产物 `.c` 被保留 | 查看错误信息里给出的 `.c` 文件路径（或设 `AOXN_DUMP_C=1` 把 C 文本打到 stderr），附最小复现提交 issue |
| 找不到 clang | clang 不在 `PATH` | 设 `AOXN_CLANG` 指向 clang 可执行文件 |
| `no available targets are compatible with triple arm64-apple-darwin` | 老版本只注册了 X86 后端 | v0.26.2 起自举侧也注册了 AArch64；升级到当前版本 |
| 手写 C++ 探针编译失败（"expected Clang 19.0.0 or newer"） | 新版 MSVC STL 拒绝 clang 18 | 与 Aoxn 产物无关；探针里手动声明 extern，别 include C++ 头文件 |

前置条件的完整列表见 [快速上手](Getting-Started.md)。

### 2. 词法与语法报错

| 报错 | 原因 | 处理 |
|---|---|---|
| `[lex] file:1:1: unexpected character …` | 文件带 **UTF-8 BOM**（Aoxn 按字节读源码） | 用编辑器存成"UTF-8 无 BOM"；PowerShell 5.1 的 `-Encoding UTF8` 会加 BOM |
| `[lex] …: unindent does not match any outer indentation level` | 缩进量与任意外层块都不匹配（常见于混用 Tab 与空格） | 用一致的 4 空格缩进（Tab 按 4 列计算，但不要混用） |
| `[parse] …: expected an expression, found Semi` | 行尾写了 `;` | 删掉；`;` 只出现在 `[T; N]` 类型里 |
| `[parse] …: expected an expression, found Slash` | 写了 `//`（既不是注释也不是运算符） | 注释用 `#`；整除用单个 `/` |
| `[parse] …: expected 'import', 'def' or 'struct' at top level` | 顶层写了语句 | 顶层只允许 `import` / `struct` / `def` / `extern def`；逻辑放进 `main` |
| `[parse] …: compound statements cannot appear on the same line after ':'` | `if x: while …` 之类 | 一行只能跟**简单语句**；`if n < 2: return n` 是合法的 |
| `[parse] …: empty array literals are not allowed (element type could not be inferred)` | 写了 `[]` | 给显式类型并给出元素：`xs: [int; 0]` 也不行，数组长度必须为正 |
| `[parse] …: unknown array length 'N' (length parameters must be declared in the fn header, …)` | `[int; N]` 里的 `N` 没在函数头声明 | 写成 `def f[T, N](arr: [T; N])` |
| `[parse] …: only one length parameter is supported (found 'M' as well)` | 一个函数里用了两个长度参数 | 改成等长数组，或拆成两个函数 |
| 1:1 报 `[type] …: program has no 'main' function` | 程序没有 `main`（注意这是**类型检查阶段**的诊断，不是语法错误） | 加 `def main() -> int:` |

### 3. 类型检查报错（都是实测文本）

| 报错 | 处理 |
|---|---|
| `'+' requires two int or two float operands, found (int, float)` | 显式转换：`x + 1` 与 `y + 1.0` 不要混；目前没有 `int()`/`float()` 内建，请让两侧类型一开始就一致 |
| `cannot concatenate string with int` | 拼接前先 `str(n)` |
| `'if' condition must be bool, found int` | 写 `if n != 0:` |
| `'!' requires bool, found int` | `not` 只作用于 `bool` |
| `'%' requires two int operands, found (float, float)` | `%` 只支持 `int` |
| `unary '-' requires int or float, found bool` | 一元负号不支持 `bool` |
| `function 'g' returns int but does not return a value on all paths` | 每条路径都要 `return`；`if/elif/else` 全分支 return 才算 |
| `unreachable statement after 'return'` | 删掉 `return` 之后的语句 |
| `cannot assign a value of type string to 'x: int'` | 重赋值不能改类型；换个新名字（注意不能遮蔽，建议换个语义清晰的名字） |
| `print requires int, float, bool, or string, found P` | 聚合不能直接打印；逐字段打印或写个 `to_string` 风格函数 |
| `cannot compare compound type [int; 2] with [int; 2]` | 数组/结构体不能 `==`；循环逐元素比较 |
| `len requires an array or string, found int` | `len` 只用于数组与字符串；字符串长度是**字节数** |
| `struct 'P' has no field 'y'` | 字段名拼错；构造时字段必须与声明一致 |
| `'main' cannot be declared extern` / `extern functions cannot be generic` | 换名字或加一层 Aoxn 包装函数 |

诊断格式与 `--json` 契约见 [命令行与工具链](CLI-and-Tooling.md) 第 5 节。

### 4. 语义误解（代码能跑，但和你以为的不一样）

| 现象 | 真相 |
|---|---|
| 函数里改了传进来的数组/结构体，调用方看不到 | **值语义**：参数是整份拷贝。要"改到外面"就让函数返回新值并写回：`v = vec_push(v, x)` |
| 数组越界不报错 | 下标**不做检查**（C 风格），越界是未定义行为 |
| `7 / 2` 得到 `3` | `/` 对两个 `int` 是截断整除；要浮点结果用 `7.0 / 2.0` |
| `print(1.5)` 输出 `1.500000` | 浮点默认 `%f`（6 位小数），f-string 里也一样；`{x:.2f}` 这类格式说明符会被**静默忽略**（不报错，也不起作用） |
| 字符串下标 `s[0]` 不合法 | 还没有 `char` 类型；用标准库 `str_get(s, i)` 取字节 |
| `range(3)` 单独用报未定义名字 | `range` 只是 `for` 上下文的关键字，不是函数 |
| f-string 里写单引号字符串报错 | Aoxn 没有单引号字符串；插值里的字符串也要用双引号 |
| 长跑服务的 RSS 一直涨 | 字符串拼接结果**永不释放**（无 GC）。用字节缓冲（`buf_new` + `store_u8`）或 `Vec`，见 [标准库](Standard-Library.md) |
| 两个文件里定义了同名函数 | `import` 把一切并进**一个命名空间**，重名报错 |

### 5. 运行期问题排查

1. **先看阶段耗时**：`$env:AOXN_TIME="1"`，确认是卡在 typecheck、codegen 的 passes/isel 还是链接。
2. **再看生成的 C**：`aoxn c prog.ax`（写到 stdout）或 `$env:AOXN_DUMP_C="1"`（编译时打到 stderr）。
   "输出悄悄算错"通常靠对比 IR 定位。
3. **定位到函数**：`$env:AOXN_TC_TRACE="1"` / `$env:AOXN_CG_TRACE="1"` 打印逐函数标记，编译器卡死或崩在哪个函数一目了然。
4. **程序运行时崩溃**：Aoxn 不做边界检查，先怀疑"种子值/空容器"——例如 `vec_new()` 的 `data` 是 0，
   `vec_get(v, -1)` 会解引用 NULL；再检查是否越界或用了已释放的地址。
5. **编译产物找不到**：非 Windows 平台可执行文件**没有** `.exe` 后缀（测试与脚本别写死扩展名）。

### 6. 测试与 CI 相关

| 现象 | 原因 |
|---|---|
| CI 上某个测试找不到产物 | 测试里硬编码了 `.exe` 字面量；应该用 `src/platform.rs` 的 helper |
| 自举固定点测试被跳过 | 逐字节对比目前**只在 Windows** 运行（以 `C:\Program Files\LLVM\lib\LLVM-C.lib` 存在为条件） |
| 非 ASCII 路径下自举编译器读不到文件 | 自举侧的 loader 用窄字符 `fopen`；Rust 编译器无此限制，测试 fixture 保持 ASCII |
| `cargo test` 很慢 | 每个用例都要真编译链接；迭代时可用 `AOXN_PASSES=default<O1>` 或 `default<O0>` 换更快的管线（测试本身不读 `--O*`，默认仍是 O3） |

### 7. 快问快答

**有包管理器吗？** 没有。`import` 就是全部：相对路径、单命名空间、include-once。

**有 GC 吗？** 没有。字符串拼接结果按设计不释放；需要复用就自己 `malloc`/`free`（`Vec`、`buf_new`）。

**支持哪些平台？** Tier 1 = Windows x86_64；Tier 2 = Linux x86_64、macOS arm64（CI 覆盖；Intel Mac 自 v0.27.1 起不再支持）。详见 [平台支持](Platform-Support.md)。

**怎么调用 C 库？** `extern def` 声明 + `-l`/`-L` 链接；指针用 `int` 承载，字符串是 NUL 结尾字节缓冲。见 [语言参考](Language-Reference.md) 第 15 节。

**怎么调试？** 没有断点/调试信息；用 `AOXN_TIME` / `AOXN_DUMP_IR` / trace 变量 + 打印。见第 5 节。

**为什么这么严格？** 无隐式转换、所有路径返回、无不可达代码这些规则，是为了让 AI 生成的代码可以被机械验证。想放宽要走语言提案。

**`--O1` 是"更快"吗？** 是"编译更快"，不是"跑得更快"：大输入编译时间约减半，但内联密集的代码运行时可能慢近四成（实测 `fib` 慢 38%）。

**能写 Web 服务吗？** 能，仓库里就有一个用 Aoxn 写的 HTTP/1.1 服务器与它的基准套件：见 [Web 平台](Web-Platform.md)。

**多返回值？** 不支持。用一个结构体返回（`PState`/`CState` 模式在自举代码里到处都是，见 [自举](Self-Hosting.md)）。

**怎么写"可变"容器？** 值语义 + 写回：`v = vec_push(v, x)`，结构体作为参数时要 `return` 出去。

## English

### 1. Environment and build

| Symptom | Cause | Fix |
|---|---|---|
| `cargo build` cannot find LLVM / link fails | `build.rs` did not locate LLVM | set `AOXN_LLVM_DIR` (`C:\Program Files\LLVM`, `/usr/lib/llvm-18`, `/opt/homebrew/opt/llvm@18`) |
| Still links the old LLVM after switching installs | the build script's LIBPATH is cached | touch `build.rs` or `cargo clean`, then rebuild |
| DLL load error when running the compiler | `LLVM-C.dll` is not next to the binary | `build.rs` copies it into `target/{debug,release}`; otherwise add LLVM's `bin` to `PATH` |
| `cannot find -lLLVM-C` on Linux | the distribution ships the C API inside a versioned `libLLVM-18.so` | set `AOXN_LLVM_DIR=/usr/lib/llvm-18`; use `AOXN_LLVM_LIB` if the probe needs help |
| clang not found | clang is not on `PATH` | set `AOXN_CLANG` to the executable |
| `no available targets are compatible with triple arm64-apple-darwin` | older builds registered only the X86 backend | v0.26.2 registers AArch64 on the self-hosted side too; upgrade |
| Hand-written C++ probes fail ("expected Clang 19.0.0 or newer") | the new MSVC STL rejects clang 18 | unrelated to Aoxn output; declare externs manually instead of including C++ headers |

Full prerequisites: [Getting Started](Getting-Started.md).

### 2. Lexer and parser errors

| Message | Cause | Fix |
|---|---|---|
| `[lex] file:1:1: unexpected character …` | the file starts with a **UTF-8 BOM** (sources are read byte-wise) | save as "UTF-8 without BOM"; PowerShell 5.1's `-Encoding UTF8` adds one |
| `[lex] …: unindent does not match any outer indentation level` | indentation matches no enclosing block (often mixed tabs/spaces) | use consistent 4-space indentation (a tab counts as 4 columns, but do not mix) |
| `[parse] …: expected an expression, found Semi` | a trailing `;` | remove it; `;` only appears in `[T; N]` |
| `[parse] …: expected an expression, found Slash` | you wrote `//` (neither a comment nor an operator) | comments use `#`; integer division is a single `/` |
| `[parse] …: expected 'import', 'def' or 'struct' at top level` | a statement at the top level | top level allows only `import` / `struct` / `def` / `extern def`; put logic in `main` |
| `[parse] …: compound statements cannot appear on the same line after ':'` | e.g. `if x: while …` | only **simple** statements may follow `:`; `if n < 2: return n` is fine |
| `[parse] …: empty array literals are not allowed (element type could not be inferred)` | you wrote `[]` | array literals need at least one element; lengths must be positive |
| `[parse] …: unknown array length 'N' (length parameters must be declared in the fn header, …)` | `N` in `[int; N]` was never declared | declare it: `def f[T, N](arr: [T; N])` |
| `[parse] …: only one length parameter is supported (found 'M' as well)` | two length parameters in one function | use equal-length arrays, or split the function |
| `[type] …: program has no 'main' function` at 1:1 | no `main` (note this is a **type-check** diagnostic, not a syntax error) | add `def main() -> int:` |

### 3. Type errors (observed text)

| Message | Fix |
|---|---|
| `'+' requires two int or two float operands, found (int, float)` | make both sides the same type; there are no `int()`/`float()` builtins yet |
| `cannot concatenate string with int` | wrap the number in `str(n)` |
| `'if' condition must be bool, found int` | write `if n != 0:` |
| `'!' requires bool, found int` | `not` only applies to `bool` |
| `'%' requires two int operands, found (float, float)` | `%` is int-only |
| `unary '-' requires int or float, found bool` | negating a `bool` is not allowed |
| `function 'g' returns int but does not return a value on all paths` | return on every path; an `if/elif/else` where all branches return counts |
| `unreachable statement after 'return'` | delete the statements after `return` |
| `cannot assign a value of type string to 'x: int'` | re-assignment cannot change the type; pick a new name (shadowing is not allowed either) |
| `print requires int, float, bool, or string, found P` | aggregates cannot be printed; print field by field or write a `to_string`-style function |
| `cannot compare compound type [int; 2] with [int; 2]` | arrays/structs have no `==`; compare element by element |
| `len requires an array or string, found int` | `len` works on arrays and strings only; string length is in **bytes** |
| `struct 'P' has no field 'y'` | typo in a field name; construction must match the declaration |
| `'main' cannot be declared extern` / `extern functions cannot be generic` | rename, or add a thin Aoxn wrapper |

Diagnostic formats and the `--json` contract: [CLI and Tooling](CLI-and-Tooling.md) §5.

### 4. Semantic misunderstandings (the code runs, but not like you think)

| Symptom | Truth |
|---|---|
| Mutating an array/struct inside a function does not reach the caller | **value semantics**: parameters are whole copies. Return the new value and assign it back: `v = vec_push(v, x)` |
| Out-of-bounds array access does not complain | indexing is **unchecked** (C-style); out-of-bounds is undefined behaviour |
| `7 / 2` gives `3` | `/` on two `int`s is truncating division; use `7.0 / 2.0` for a float |
| `print(1.5)` prints `1.500000` | floats default to `%f` (6 decimals), f-strings included; a specifier like `{x:.2f}` is **silently ignored** (no error, no effect) |
| `s[0]` is rejected | there is no `char` type yet; use the stdlib `str_get(s, i)` for bytes |
| `range(3)` alone is an undefined name | `range` is a `for`-context keyword, not a function |
| A single-quoted string inside an f-string is an error | Aoxn has no single-quoted strings; inner strings use double quotes too |
| A long-running service's RSS keeps growing | concatenation results are **never freed** (no GC). Use byte buffers (`buf_new` + `store_u8`) or `Vec` — see [Standard Library](Standard-Library.md) |
| Two files defining the same function fail | `import` merges everything into **one namespace**; duplicate names are errors |

### 5. Runtime investigation

1. **Stage timings first**: `$env:AOXN_TIME="1"` tells you whether you are stuck in
   typecheck, in the codegen passes/isel, or in the link.
2. **Then the IR**: `aoxn ir prog.ax` (optimized) or `$env:AOXN_DUMP_IR="1"`
   (pre-verify IR). "Silently wrong output" is usually an IR-diff problem.
3. **Narrow to a function**: `$env:AOXN_TC_TRACE="1"` / `$env:AOXN_CG_TRACE="1"`
   print per-function markers, so a hang or crash points at one function.
4. **Runtime crash in the program**: there is no bounds checking — suspect seed
   values and empty containers first (`vec_new()` has `data == 0`, so
   `vec_get(v, -1)` dereferences NULL), then out-of-bounds or freed addresses.
5. **Product not found**: off Windows the executable has **no** `.exe` extension
   (do not hardcode it in scripts or tests).

### 6. Tests and CI

| Symptom | Cause |
|---|---|
| A test cannot find its product in CI | the test hardcodes `.exe` or the `LLVM-C` literal; use the `src/platform.rs` helpers |
| The self-hosting fixed-point test is skipped | the byte-exact comparison currently runs **on Windows only** (it requires `C:\Program Files\LLVM\lib\LLVM-C.lib`) |
| The self-hosted compiler cannot read a non-ASCII path | its loader uses narrow `fopen`; the Rust compiler is unaffected, and test fixtures stay ASCII |
| `cargo test` is slow | every case compiles and links for real; while iterating, switch the pipeline with `AOXN_PASSES=default<O1>` or `default<O0>` (the tests themselves never read `--O*`; O3 stays the default) |

### 7. Quick FAQ

**Is there a package manager?** No. `import` is everything: relative paths, one
namespace, include-once.

**Is there a GC?** No. Concatenation results are leaked by design; manage memory
yourself (`malloc`/`free`, `Vec`, `buf_new`).

**Which platforms are supported?** Tier 1 = Windows x86_64; Tier 2 = Linux
x86_64 and macOS arm64 (covered by CI; Intel Macs have been unsupported since
v0.27.1) — see
[Platform Support](Platform-Support.md).

**How do I call a C library?** Declare it with `extern def` and link with
`-l`/`-L`; pointers travel as `int` and strings are NUL-terminated byte buffers —
see [Language Reference](Language-Reference.md) §15.

**How do I debug?** No breakpoints or debug info; use `AOXN_TIME`,
`AOXN_DUMP_IR`, the trace variables and prints (section 5).

**Why is it so strict?** No implicit conversions, all-paths-return and
no-unreachable-code exist so machine-generated code can be verified
mechanically. Relaxing one is a language proposal.

**Is `--O1` "faster"?** It compiles faster, it does not run faster: about half
the compile time on large inputs, at the cost of slower inlining-heavy code
(`fib` measured 38% slower at O1).

**Can it write web services?** Yes — the repository ships an HTTP/1.1 server
written in Aoxn plus its benchmark suite: [Web Platform](Web-Platform.md).

**Multiple return values?** Not supported. Return a struct (the `PState` /
`CState` write-back pattern is everywhere in the self-hosted compiler — see
[Self-Hosting](Self-Hosting.md)).

**How do I write a "mutable" container?** Value semantics plus write-back:
`v = vec_push(v, x)`, and struct parameters must be returned.

---

## 源文件 / Source files

- [src/lexer.rs](../src/lexer.rs), [src/parser.rs](../src/parser.rs), [src/typecheck.rs](../src/typecheck.rs) — the diagnostic messages quoted above
- [src/lib.rs](../src/lib.rs) — diagnostic formatting, clang lookup, `AOXN_TIME`
- [src/main.rs](../src/main.rs) — exit codes, cache behavior, CLI errors
- [src/platform.rs](../src/platform.rs) — executable extension and link-name probing
- [build.rs](../build.rs) — LLVM discovery and the `LLVM-C.dll` copy
- [docs/platform-support.md](../docs/platform-support.md) — platform-specific failure history (T1–T5)
- [CONTRIBUTING.md](../CONTRIBUTING.md) — debug switches and source conventions
