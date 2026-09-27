# 术语表 · Glossary

> **中文**：Aoxn 编译器仓库中反复出现、名字本身不足以自解释的术语清单，每条给出精确定义与出处文件。
> **English**: The terms that recur across the Aoxn compiler repository and whose names alone do not explain them — each with a precise definition and its source file.

## 中文

本页按主题分组，每张表都用同一组列：**术语 / 中文说明 / English / 出处文件**。所有定义都以 v0.26.3 的源码、测试与
`CHANGELOG.md` 为准；凡涉及"已实现 / 尚未实现"的措辞，都与 [路线图](Roadmap.md) 保持一致。表中的文件链接指向仓库
中的真实文件，行号是 v0.26.3 的当前行号。

### 一、语言、文件与平台定位

| 术语 | 中文说明 | English | 出处文件 |
|---|---|---|---|
| **Aoxn（原名 Axon）** | 本仓库实现的 AI 原生、静态类型、AOT 编译语言；v0.10.0 从 Axon 改名为 Aoxn（crate、二进制、文档、示例同批改名），二者是同一门语言的新旧名。 | Aoxn (formerly Axon) | [CHANGELOG.md](../CHANGELOG.md)、[README.md](../README.md) |
| **`.ax`** | Aoxn 源文件的扩展名。源文件按**字节**读取，所以写入 UTF-8 BOM（`EF BB BF`）会在 1:1 报 "unexpected character"。 | the `.ax` source extension | [CONTRIBUTING.md](../CONTRIBUTING.md)、[src/lexer.rs](../src/lexer.rs) |
| **`aoxn`（CLI）** | 编译器二进制，子命令 `build` / `run` / `ir`；`-o`、`--json`、`-l`/`-L`、`--O0..--O3`、`--cpu`。`run` 与 `build` 共享一份构建缓存。 | the `aoxn` CLI | [src/main.rs](../src/main.rs) |
| **Tier 1 / Tier 2** | 平台支持等级：Tier 1 = Windows x86_64（完全支持，MSVC Build Tools + winget LLVM）；Tier 2 = Linux x86_64、macOS x86_64、macOS arm64（CI 四平台矩阵验证）。交叉编译、MinGW、32 位目标不在范围内。 | Tier 1 / Tier 2 platforms | [docs/spec.md](../docs/spec.md)、[docs/platform-support.md](../docs/platform-support.md) |

### 二、词法与语法

| 术语 | 中文说明 | English | 出处文件 |
|---|---|---|---|
| **layout tokens（`Newline` / `Indent` / `Dedent`）** | 词法器维护一个缩进栈，把 Python 式布局变成显式 token：块开始发 `Indent`、块结束发 `Dedent`、语句结束发 `Newline`；空行与纯注释行**不产生任何 token**，EOF 时先补 `Newline`、再补未闭合的 `Dedent`、最后发 `Eof`。 | layout tokens NEWLINE/INDENT/DEDENT | [src/lexer.rs](../src/lexer.rs)、[docs/spec.md](../docs/spec.md) |
| **隐式续行（implicit line joining）** | 只要 `paren_depth > 0`（位于任意括号内），换行与缩进就被忽略，因此调用参数可以跨行书写、允许尾随逗号；括号不配对会报 "unclosed bracket"。 | implicit line joining | [src/lexer.rs](../src/lexer.rs)、[docs/spec.md](../docs/spec.md) |
| **f-string 脱糖** | f-string 在词法阶段被拆成 `FStrPart` 列表（字面量 + 插值 token 序列），插值在主字符缓冲区上**原地**重新词法化（虚拟括号关闭缩进跟踪、保留列号）；解析阶段脱糖成 `"lit" + str(expr) + ...` 链。 | f-string desugaring | [src/lexer.rs](../src/lexer.rs)、[CHANGELOG.md](../CHANGELOG.md) |

### 三、类型、泛型与单态化

| 术语 | 中文说明 | English | 出处文件 |
|---|---|---|---|
| **`GENERIC_LEN` 哨兵** | 数组长度里的哨兵值 `usize::MAX`，表示 `[T; N]` 中由调用点推断的长度参数（每个函数最多一个长度参数）。泛型声明本身绝不进入 codegen——对这个哨兵构造数组类型会让 LLVM 试图建 40 亿个元素而崩溃。 | the `GENERIC_LEN` sentinel | [src/ast.rs](../src/ast.rs)、[src/parser.rs](../src/parser.rs)、[src/lib.rs](../src/lib.rs) |
| **单态化（monomorphization）** | 每个不同的（类型, 长度）组合在编译期为泛型函数生成一个专用实例：调用点实参类型与声明形参类型统一（unify）→ 克隆声明 AST 并替换类型参数与 `N` → 实例入队（FIFO `VecDeque`）检查函数体。实例是**检查与发射共用的同一批对象**，中途克隆会让实例体内的泛型调用丢失路由。 | monomorphization | [src/typecheck.rs](../src/typecheck.rs)、[CHANGELOG.md](../CHANGELOG.md) |
| **实例名修饰（mangled instance name）** | 实例名 = 函数名 + 每个类型参数一个 slug（`i` / `f` / `b` / `s` / `v`，数组是 `a<元素>x<长度>`，结构体是 `s_<名字>`，函数体未用到则记 `?`）+ 长度，例如 `sort.i.8`、`first.i.?.3`。 | mangled instance names | [src/typecheck.rs](../src/typecheck.rs)、[CHANGELOG.md](../CHANGELOG.md) |
| **`call_map`** | 从泛型调用表达式的 **AST 节点地址**到实例修饰名的映射；codegen 在发射调用时用它把调用点指向实例。这与临时缓存用同一套"节点地址"键，因此克隆 AST 会同时破坏两者。 | `call_map` | [src/typecheck.rs](../src/typecheck.rs)、[src/codegen.rs](../src/codegen.rs) |
| **`FnSig` / `StructTable`** | 检查器的签名表（`FnSig { params, ret }`）与结构体表。v0.26.3 起 `StructTable` 的值是 `StructInfo { fields, index }`，字段查找从线性扫描变成 O(1)，重复/缺失字段的诊断文案与错误位置逐字节不变。 | `FnSig` / `StructTable` | [src/typecheck.rs](../src/typecheck.rs)、[CHANGELOG.md](../CHANGELOG.md) |

### 四、值语义与代码生成

| 术语 | 中文说明 | English | 出处文件 |
|---|---|---|---|
| **值语义（value semantics）** | 数组与结构体是一等值类型：赋值、传参、返回都复制整个值（降低为 `memcpy`），语言里没有引用/指针；`b = a; b[0] = 99` 不会改变 `a`。 | value semantics | [docs/spec.md](../docs/spec.md)、[src/codegen.rs](../src/codegen.rs) |
| **聚合（aggregate）** | 结构体与数组这类复合类型。在发射的表达式里聚合**一律以地址表示**（`emit_expr` 对复合类型返回指针，`emit_aggregate_ptr` 只做转发），复制永远是显式 `memcpy`，绝不把整个聚合 load/store 成 SSA 值。 | aggregate | [src/codegen.rs](../src/codegen.rs)、[CHANGELOG.md](../CHANGELOG.md) v0.26.0 |
| **聚合 ABI（指针传参 + sret）** | v0.26.0 起的函数边界约定：结构体/数组参数按**指针**传递，被调者把它 `memcpy` 进自己的栈槽（值语义不变）；聚合返回值走 **sret**（隐藏的首个出参指针，真正返回 `void`），调用点需要的信息由 `FnInfo { params, sret }` 承载。`extern def` 保持普通 C ABI，因为那是 FFI 边界。 | aggregate ABI (pointer parameters + sret) | [CHANGELOG.md](../CHANGELOG.md)、[CONTRIBUTING.md](../CONTRIBUTING.md) |
| **entry-hoisted alloca / `alloca_in_entry`** | 所有 alloca 必须插在 entry 基本块的**最前面**：发射 `let` 时 entry 可能已经被短路分支终结，所以只能用 `Gen::alloca_in_entry`（定位到首条指令之前），不能"定位到 entry 末尾"。 | entry-hoisted alloca / `alloca_in_entry` | [src/codegen.rs](../src/codegen.rs) |
| **临时缓存（`lit_temps` / `val_temps` / `iter_temps`）** | 三张以 **AST 节点地址**为键的缓存，让循环体复用同一个入口提升的 alloca：`lit_temps` 给数组/结构体字面量与聚合返回值，`val_temps` 给非左值聚合基址（如 `f()[0]`），`iter_temps` 给 `[e] * N` 复制循环的迭代槽。绝不能把**克隆**出来的 `Expr` 传进去——地址会被分配器回收，之后别处的节点会命中同一个临时（v0.26.0 修过这种跨函数别名："Referring to an instruction in another function!"）。 | temp caches (`lit_temps`/`val_temps`/`iter_temps`) | [src/codegen.rs](../src/codegen.rs)、[CHANGELOG.md](../CHANGELOG.md) |
| **字符串长度缓存（`str_lens` / `len_slots` / `invalidate_str_lens`）** | `str_lens` 记录已知字节长度的 IR 值（字面量、拼接结果、`str()`/`as_string()` 结果），`len_slots` 为字符串变量保存一个长度 alloca 并在每次 Let/Assign/for 元素复制时更新；`str_len_of` 兜底调用 `strlen`。它让 `s = s + piece` 累加循环是 O(总字节数) 而不是 O(n²)。不变量：任何原始写内存或未经白名单的 C 调用之前必须 `invalidate_str_lens()`——缓存建立在"字符串不可变"的假设上。 | string length caching | [src/codegen.rs](../src/codegen.rs)、[CHANGELOG.md](../CHANGELOG.md) v0.20.0 |
| **`nsw` / `inbounds`** | `int` 算术发射 `nsw`（无有符号回绕）、数组/字段 GEP 发射 `inbounds`——两者都依赖规范里"有符号整数溢出与越界索引是未定义行为（与 C 一致）"的承诺。原始内存内建（`load_u8`/`store_u8`）接受任意地址，必须保持**非** inbounds。 | `nsw` / `inbounds` | [src/codegen.rs](../src/codegen.rs)、[docs/spec.md](../docs/spec.md) |
| **原始内存内建（raw memory builtins）** | `load_i64`/`store_i64`、`load_f64`/`store_f64`、`load_u8(base, off)`/`store_u8(base, off, v)`（`base` 可以是 `int` 地址或 `string` 字节）、`as_string`/`as_ptr` 指针重解释。地址就是普通 `int`，不做任何检查——标准库与自举的逃生舱，unsafe by design。 | raw memory builtins | [docs/spec.md](../docs/spec.md)、[stdlib/stdlib.ax](../stdlib/stdlib.ax) |

### 五、诊断、多文件加载与 FFI

| 术语 | 中文说明 | English | 出处文件 |
|---|---|---|---|
| **`Diag` 与 stage** | 统一诊断结构 `Diag { stage, file: u32, line, col, message }`；`stage` 取 `lex` / `parse` / `type` / `internal` / `link` / `io` 之一，`file` 是编译期文件登记表的下标（打印或 `--json` 时才解析成文件名，JSON 里换行与控制字符会被转义）。编译器内部故障必须以 `internal` 诊断呈现，而不是 panic（LLVM 自身的 fatal error 除外）。 | `Diag` and its stage | [src/lib.rs](../src/lib.rs)、[src/files.rs](../src/files.rs) |
| **include-once 与 import 循环检测** | `load_program` 递归解析 `import "..."`，路径相对**导入者所在文件**：每个规范化路径只包含一次，循环导入用导入栈检测并以完整链条报错（`circular import: a -> b -> a`，stage 为 `io`）。自举侧在 `selfhost/load.ax` 里实现同一套语义。 | include-once and import cycle detection | [src/lib.rs](../src/lib.rs)、[selfhost/load.ax](../selfhost/load.ax) |
| **`extern def`（FFI）** | 无函数体的 C 运行时声明，链接期从默认库解析；检查器强制两条限制：不能命名为 `main`、不能使用泛型。标准库用它接入 `sqrt`/`floor`/`ceil`、`malloc`/`realloc`/`free`/`memcpy`、`fopen` 系列与 `system`——其中 `fopen`/`system` 就是 `string` 参数。**`docs/spec.md` 说 extern "不能用 `string` 参数/返回值" 已过时**：实测 `extern def strlen(s: string) -> int` 可编译、可运行。指针以 `int` 传递（x86-64 上 ABI 相同）。 | `extern def` (C FFI) | [stdlib/stdlib.ax](../stdlib/stdlib.ax)、[src/typecheck.rs](../src/typecheck.rs) |

### 六、优化、缓存与平台

| 术语 | 中文说明 | English | 出处文件 |
|---|---|---|---|
| **优化级别 `--O0..--O3`** | `--O0` 不跑 IR 流水线且 TargetMachine 用 `LLVMCodeGenLevelNone`（走 fast-isel）；`--O1`/`--O2`/`--O3` 分别选 `default<O1>`/`default<O2>`/`default<O3>` 流水线，默认 O3（"与 `clang -O3` 同性能"的承诺）。最多给一个级别标志，否则以退出码 2 结束。 | optimization levels `--O0..--O3` | [src/main.rs](../src/main.rs)、[src/codegen.rs](../src/codegen.rs) |
| **`AOXN_PASSES`** | 覆盖 LLVM pass 流水线文本的环境变量，在任意 `> --O0` 的级别生效（例如 `AOXN_PASSES=default<O1>`）；它也是构建缓存键的一部分，改了它就会 miss。 | `AOXN_PASSES` | [src/main.rs](../src/main.rs)、[docs/spec.md](../docs/spec.md) |
| **内容哈希缓存（cache key）** | `aoxn run` 与 `aoxn build` 共用的一份可执行文件缓存：键覆盖 entry **及全部传递导入**的文件内容、编译器二进制身份（大小 + mtime）、所有影响代码生成的选项（级别、`AOXN_CPU`、`AOXN_PASSES`、`-l`/`-L`、解析出的 clang 路径）。默认目录 `target/cache`（`AOXN_CACHE_DIR` 迁移、`AOXN_NO_CACHE=1` 关闭），上限 64 条、按 mtime 近似 LRU；命中时 `run` 直接执行缓存产物、`build` 复制到 `-o`。 | content-hash cache key | [src/main.rs](../src/main.rs)、[CHANGELOG.md](../CHANGELOG.md) v0.26.2 / v0.26.3 |
| **`platform::llvm_link_name()`** | 按平台探测 LLVM C API 的 `-l` 名字：Windows/macOS 是 `LLVM-C`，Debian/Ubuntu 把 C API 放进版本化的 `libLLVM-<N>.so`（返回最新的 `LLVM-<N>`），否则回退到 `LLVM`；`AOXN_LLVM_LIB` 可覆盖。硬编码 `-lLLVM-C` 曾在 Linux CI 上报 `cannot find -lLLVM-C`。 | `platform::llvm_link_name()` | [src/platform.rs](../src/platform.rs)、[docs/platform-support.md](../docs/platform-support.md) |
| **`target_os()`** | 编译期内建函数，返回 `"windows"` / `"linux"` / `"macos"` / `"other"`，折叠为模块内私有字符串常量，**不是**运行时系统调用。两侧实现（Rust 的 `platform::target_os_name()` 与自举的 `selfhost/codegen.ax`）必须折叠出同一个值，否则逐字节固定点破裂。目前它是语言里唯一的平台感知设施。 | `target_os()` | [docs/spec.md](../docs/spec.md)、[src/codegen.rs](../src/codegen.rs) |
| **COFF / ELF / Mach-O** | 目标文件格式由目标三元组决定，三种格式走同一条 `LLVMTargetMachineEmitToFile(..., 1)` 路径（`1` = 目标文件，`0` 是汇编）。固定点对比在 Windows 上是 COFF，在其他平台应变成 ELF/Mach-O 对比，对比逻辑不变。 | COFF / ELF / Mach-O | [docs/platform-support.md](../docs/platform-support.md)、[CONTRIBUTING.md](../CONTRIBUTING.md) |
| **PIE 与 `RELOC_PIC`** | Linux/macOS 默认链接 PIE，因此 TargetMachine 在非 Windows 上用 `RELOC_PIC`（2）、Windows 上用 `RELOC_DEFAULT`（0）；自举侧的对应常量是 `CG_RELOC()`。 | PIE and `RELOC_PIC` | [src/llvm.rs](../src/llvm.rs)、[src/codegen.rs](../src/codegen.rs)、[docs/platform-support.md](../docs/platform-support.md) |
| **rpath** | POSIX 上 `link_opts` 会为每个 `-L` 目录追加 `-Wl,-rpath,<dir>`：链接到非默认 LLVM 目录时（Homebrew 的 `libLLVM-C.dylib` 只是再导出 `@rpath/libLLVM.dylib`），没有 rpath 的可执行文件起不来。Windows 链接器不接受 `-rpath`，所以该分支只在 POSIX 生效。 | rpath | [CHANGELOG.md](../CHANGELOG.md) v0.26.2、[src/lib.rs](../src/lib.rs) |

### 七、自举（bootstrap）

| 术语 | 中文说明 | English | 出处文件 |
|---|---|---|---|
| **bootstrap 与 stage 0/1/2** | 自举的分级：stage 0 = 现在的 Rust 编译器；stage 1 = 用 Aoxn 写的编译器；stage 2 = 由 stage 1 编译出的编译器（产物 `target/selfhost_stage2.exe`）。stage 3 = CI 里长期保持固定点绿。 | bootstrap and stages 0/1/2 | [docs/selfhost.md](../docs/selfhost.md) |
| **固定点（fixed point）** | 自举的验收形态：Aoxn 写的 driver 编译**整个自举编译器**（约 7k 行：driver + codegen + typecheck + parser + lexer + loader + stdlib）得到 stage-2，再由 stage-2 编译同一程序，两侧产出的 **IR 与目标文件逐字节一致**（v0.22 只比 stdout/退出码，v0.24 加 IR 字节，v0.25 加对象字节）。 | fixed point (byte-identical IR + object) | [CHANGELOG.md](../CHANGELOG.md) v0.22 / v0.24 / v0.25 |
| **`selfhost_driver_self_compiles`** | 承载固定点的端到端测试：先构建 `selfhost/driver_self_demo.ax`，运行它产出 stage-2，再让 stage-2 编译 `STDLIB_USE_PROG`，最后对比两侧 IR 与对象字节。当 `C:/Program Files/LLVM/lib/LLVM-C.lib` 不存在时它**自行跳过**，所以逐字节固定点目前只在 Windows 上实际执行。 | `selfhost_driver_self_compiles` | [tests/pipeline.rs](../tests/pipeline.rs) |
| **bootstrap crutch（拐杖）** | 自举期间 Rust 编译器的角色：在 stage 2 稳定之前它是必须一直可用的"拐杖"，之后退化为纯测试预言机（oracle）；一个语言新特性要先在 Rust 侧实现，才能被自举侧移植。 | bootstrap crutch | [docs/selfhost.md](../docs/selfhost.md) |

### 八、Web 基准与性能解读

| 术语 | 中文说明 | English | 出处文件 |
|---|---|---|---|
| **`bb_*` 字节缓冲** | `web/http_buf.ax` 里的字节缓冲追加助手（`bb_byte` / `bb_crlf` / `bb_str` / `bb_int`，都返回**新长度**）。长跑服务器必须把响应渲染进可复用缓冲区：Aoxn 字符串不可变、拼接结果按设计不释放，逐请求拼接会让 RSS 膨胀。因此 HTTP 的 CRLF 是手写的字节 13/10（语言没有 `\r` 转义）。 | the `bb_*` byte-buffer helpers | [web/http_buf.ax](../web/http_buf.ax)、[web/README.md](../web/README.md) |
| **oha** | 基准套件使用的负载生成器（oha 1.16.0，PGO 构建），32 条 keep-alive 连接；项目明确不用 autocannon——单核客户端约 4k req/s 就封顶，会掩盖服务器差异。 | oha | [docs/web-benchmark.md](../docs/web-benchmark.md)、[web/README.md](../web/README.md) |
| **parity 测试** | 两个用法：① `web/loadtest/parity.mjs`——Aoxn 服务器与 Node 参考实现三个路由响应体逐字节一致、404、keep-alive、`/metrics` 的功能对等测试；② 性能承诺 "parity with `clang -O3`"，由 `examples/bench_*.ax` 的同算法对比支撑。 | parity test / `clang -O3` parity | [web/loadtest/parity.mjs](../web/loadtest/parity.mjs)、[docs/spec.md](../docs/spec.md) |
| **Little's law 式在途请求解读** | 用 `在途请求数 ≈ req/s × 平均延迟` 从吞吐与延迟反推服务器是否饱和：Aoxn 在 32 连接下只有约 0.7 个在途请求（>95% 时间空闲，说明测到的 7.6k req/s 是压测客户端上限），Node.js 与 Next.js 约 28（连接在排队）。所以区分服务器要看延迟与在途数，而不是原始 rps。 | in-flight requests read via Little's law | [docs/web-benchmark.md](../docs/web-benchmark.md) |

### 常见混淆

1. **`//` 不是注释，而是整除**（注释只有 `#`）。写 `x = 6 // 2` 得到 3，写 `// 注释` 是语法错误。
2. **`[T; N]` 里的 `;` 是数组类型语法的一部分**，不是语句终止符——Aoxn 没有语句级分号，`;` 只出现在这里。
3. **"没有 GC" 指字符串拼接结果不释放**（不可变字符串、按设计泄漏），不是"语言没有内存管理"：标准库的 `Vec`、
   `buf_new`、`read_file` 都直接调用 `malloc`/`realloc`/`free`，释放责任在调用者。
4. **`--O1` 不是"让程序更快"，而是"让编译更快"**：它跑 `default<O1>`，在递归/内联密集的代码（fib）上运行时约慢
   38%，循环型代码基本不受影响——这正是 O3 保持默认的原因。
5. **Aoxn 与 Axon 是同一门语言的新旧名**（v0.10.0 改名）。`AXON_LLVM_DIR` 是保留的旧环境变量别名，会被兼容读取；
   但 `AXON_DUMP_IR` 并不在源码中——代码只读 `AOXN_DUMP_IR`。
6. **`README.md` 的 Status 段与 `docs/spec.md` 是过时的**：前者仍写 "v0.7 · 70/70 tests"，后者标称 v0.9，且其中还有两处与
   实现不符——Statements 段说"`while` 是唯一的循环（还没有 `for`）"，而 `for` 早已实现；Program structure 段说 extern
   不能用 `string` 参数，而 stdlib 的 `fopen`/`system` 正是这么用的。真实基线是 v0.26.3 与 97 个端到端测试，版本历史的
   权威来源是 `CHANGELOG.md`。

## English

This page groups terms by topic; every table uses the same four columns: **Term / 中文说明 / Definition (English) / Source**.
All definitions follow the v0.26.3 sources, tests, and `CHANGELOG.md`; where a term is marked implemented or not implemented,
that matches the [Roadmap](Roadmap.md). File links point at real files in the repository, and the line numbers are
v0.26.3 line numbers.

### 1. Language, files, and platform positioning

| Term | 中文说明 | Definition (English) | Source |
|---|---|---|---|
| **Aoxn (formerly Axon)** | 本仓库实现的 AI 原生、静态类型、AOT 编译语言；v0.10.0 从 Axon 改名而来。 | The AI-native, statically typed, ahead-of-time compiled language this repository implements; renamed from Axon to Aoxn in v0.10.0 (crate, binary, docs and examples in one change) — the two names refer to the same language, old and new. | [CHANGELOG.md](../CHANGELOG.md), [README.md](../README.md) |
| **`.ax`** | Aoxn 源文件扩展名；源文件按字节读取。 | The Aoxn source-file extension. Sources are read **byte-wise**, so a UTF-8 BOM (`EF BB BF`) is reported as "unexpected character" at 1:1. | [CONTRIBUTING.md](../CONTRIBUTING.md), [src/lexer.rs](../src/lexer.rs) |
| **`aoxn` (CLI)** | 编译器二进制；`build` / `run` / `ir` 三个子命令。 | The compiler binary with the `build` / `run` / `ir` subcommands plus `-o`, `--json`, `-l`/`-L`, `--O0..--O3` and `--cpu`. `run` and `build` share one build cache. | [src/main.rs](../src/main.rs) |
| **Tier 1 / Tier 2 platforms** | Tier 1 = Windows x86_64；Tier 2 = Linux x86_64 与两种 macOS。 | Support tiers: Tier 1 is Windows x86_64 (fully supported: MSVC Build Tools + winget LLVM); Tier 2 is Linux x86_64, macOS x86_64 and macOS arm64, verified by the four-platform CI matrix. Cross-compilation, MinGW and 32-bit targets are out of scope. | [docs/spec.md](../docs/spec.md), [docs/platform-support.md](../docs/platform-support.md) |

### 2. Lexing and grammar

| Term | 中文说明 | Definition (English) | Source |
|---|---|---|---|
| **layout tokens (`Newline` / `Indent` / `Dedent`)** | 缩进栈把 Python 式布局变成显式 token；空行与纯注释行不产生 token。 | The lexer keeps an indent stack and turns Python-style layout into explicit tokens: `Indent` opens a block, `Dedent` closes one, `Newline` ends a statement. Blank and comment-only lines produce **no tokens at all**; at EOF a trailing `Newline` is flushed, then the pending `Dedent`s, then `Eof`. | [src/lexer.rs](../src/lexer.rs), [docs/spec.md](../docs/spec.md) |
| **implicit line joining** | 括号内换行与缩进被忽略，参数可跨行。 | While `paren_depth > 0` (inside any bracket) newlines and indentation are ignored, so call arguments may span lines and a trailing comma is allowed; an unbalanced bracket reports "unclosed bracket". | [src/lexer.rs](../src/lexer.rs), [docs/spec.md](../docs/spec.md) |
| **f-string desugaring** | f-string 拆成字面量与插值 token，插值原地重词法化，解析期脱糖为字符串拼接。 | The lexer splits an f-string into `FStrPart`s (literal chunks plus interpolation token streams) and re-lexes each interpolation **in place** over the main character buffer (a virtual paren disables indent tracking, the column offset is preserved); the parser then desugars the whole thing into a `"lit" + str(expr) + ...` chain. | [src/lexer.rs](../src/lexer.rs), [CHANGELOG.md](../CHANGELOG.md) |

### 3. Types, generics, and monomorphization

| Term | 中文说明 | Definition (English) | Source |
|---|---|---|---|
| **the `GENERIC_LEN` sentinel** | 数组长度哨兵 `usize::MAX`，表示由调用点推断的 `N`；泛型声明绝不进入 codegen。 | The `usize::MAX` sentinel used as an array length to mark the caller-inferred length parameter of `[T; N]` (at most one length parameter per function). Generic declarations **never reach codegen**: building an array type from the sentinel makes LLVM try to construct a four-billion-element type and crash. | [src/ast.rs](../src/ast.rs), [src/parser.rs](../src/parser.rs), [src/lib.rs](../src/lib.rs) |
| **monomorphization** | 每个（类型, 长度）组合生成一个专用实例：统一 → 克隆替换 → 入队检查。 | Every distinct (type, length) combination gets a dedicated instance at compile time: argument types are unified against the declared parameter types, the declaration's AST is cloned with the type parameters and `N` substituted, and the instance's body is queued (a FIFO `VecDeque`) for checking. Instances are **the same objects that codegen emits** — cloning between checking and emission loses the routing of generic calls inside instance bodies. | [src/typecheck.rs](../src/typecheck.rs), [CHANGELOG.md](../CHANGELOG.md) |
| **mangled instance names** | 实例名 = 函数名 + 类型参数 slug + 长度，如 `sort.i.8`。 | An instance name is the function name plus one slug per type parameter (`i` / `f` / `b` / `s` / `v`, `a<elem>x<len>` for arrays, `s_<Name>` for structs, `?` when the body never uses the parameter) plus the length — e.g. `sort.i.8`, `first.i.?.3`. | [src/typecheck.rs](../src/typecheck.rs), [CHANGELOG.md](../CHANGELOG.md) |
| **`call_map`** | 泛型调用表达式 AST 节点地址 → 实例修饰名。 | A map from the **AST node address** of a generic call expression to the mangled instance name; codegen looks call sites up there instead of emitting the generic declaration. It shares the "node address" keying with the temp caches, so cloning AST nodes breaks both. | [src/typecheck.rs](../src/typecheck.rs), [src/codegen.rs](../src/codegen.rs) |
| **`FnSig` / `StructTable`** | 签名表与结构体表；结构体字段查找 O(1)。 | The checker's signature table (`FnSig { params, ret }`) and struct table. Since v0.26.3 a `StructTable` value is a `StructInfo { fields, index }`, making field lookup O(1) instead of a linear scan while duplicate/missing-field diagnostics keep byte-identical messages and positions. | [src/typecheck.rs](../src/typecheck.rs), [CHANGELOG.md](../CHANGELOG.md) |

### 4. Value semantics and code generation

| Term | 中文说明 | Definition (English) | Source |
|---|---|---|---|
| **value semantics** | 赋值/传参/返回复制整个值（memcpy），没有引用。 | Arrays and structs are first-class value types: assignment, parameter passing and returns copy the whole value (lowered to `memcpy`), and the language has no references or pointers — `b = a; b[0] = 99` leaves `a` untouched. | [docs/spec.md](../docs/spec.md), [src/codegen.rs](../src/codegen.rs) |
| **aggregate** | 结构体与数组；发射时一律以地址表示，复制是显式 memcpy。 | A compound type (struct or array). In emitted expressions an aggregate is **represented by its address** (`emit_expr` returns a pointer for compound types and `emit_aggregate_ptr` merely forwards it); copying is always an explicit `memcpy`, and a whole aggregate is never loaded or stored as a single SSA value. | [src/codegen.rs](../src/codegen.rs), [CHANGELOG.md](../CHANGELOG.md) v0.26.0 |
| **aggregate ABI (pointer parameters + sret)** | 聚合参数按指针传、被调者 memcpy 进本地槽；聚合返回走 sret。 | The function-boundary convention since v0.26.0: struct and array parameters cross the boundary as a **pointer** that the callee `memcpy`s into its own stack slot (value semantics preserved), and aggregate returns use an **sret** out-pointer (a hidden first parameter with a real `void` return); `FnInfo { params, sret }` carries what call sites need. `extern def` keeps the plain C ABI, because that is the FFI boundary. | [CHANGELOG.md](../CHANGELOG.md), [CONTRIBUTING.md](../CONTRIBUTING.md) |
| **entry-hoisted alloca / `alloca_in_entry`** | 所有 alloca 必须插在 entry 块最前面。 | Every alloca must be inserted at the **top** of the entry block: by the time a `let` is emitted the entry block may already be terminated by short-circuit branches, so only `Gen::alloca_in_entry` (position before the first instruction) is correct — never "position at the end of entry". | [src/codegen.rs](../src/codegen.rs) |
| **temp caches (`lit_temps` / `val_temps` / `iter_temps`)** | 以 AST 节点地址为键，使循环体复用同一个入口提升的 alloca。 | Three caches keyed by **AST node address** so that a loop body reuses one entry-hoisted alloca: `lit_temps` for array/struct literals and aggregate-returning calls, `val_temps` for non-lvalue aggregate bases such as `f()[0]`, `iter_temps` for the `[e] * N` fill loop's induction slot. Never hand them a **cloned** `Expr`: the allocator recycles a temporary's address and a later site then reuses a foreign temp (the cross-function aliasing fixed in v0.26.0: "Referring to an instruction in another function!"). | [src/codegen.rs](../src/codegen.rs), [CHANGELOG.md](../CHANGELOG.md) |
| **string length caching (`str_lens` / `len_slots` / `invalidate_str_lens`)** | 记录已知字符串字节长度，使累加拼接为 O(总字节)。 | `str_lens` maps IR values to known byte lengths (literals, concat results, `str()`/`as_string()` results) and `len_slots` keeps a length alloca per string variable, updated on every Let/Assign/for-element copy; `str_len_of` falls back to `strlen`. This is what keeps an `s = s + piece` accumulator loop O(total bytes) instead of O(n²). Invariant: any raw store or non-whitelisted C call must run `invalidate_str_lens()` first, because the cache assumes string immutability. | [src/codegen.rs](../src/codegen.rs), [CHANGELOG.md](../CHANGELOG.md) v0.20.0 |
| **`nsw` / `inbounds`** | `int` 算术发 nsw、聚合 GEP 发 inbounds，都依赖规范的 UB 承诺。 | `int` arithmetic emits `nsw` (no signed wrap) and array/field GEPs emit `inbounds`; both rely on the spec's undefined behavior for signed overflow and out-of-bounds indexing (like C). The raw-memory builtins (`load_u8`/`store_u8`) take arbitrary addresses and must stay **non**-inbounds. | [src/codegen.rs](../src/codegen.rs), [docs/spec.md](../docs/spec.md) |
| **raw memory builtins** | 地址即 int 的 load/store 与指针重解释内建，不做检查。 | `load_i64`/`store_i64`, `load_f64`/`store_f64`, `load_u8(base, off)`/`store_u8(base, off, v)` (where `base` may be an `int` address or a `string`'s bytes) and the pointer reinterpretations `as_string`/`as_ptr`. Addresses are plain `int` and nothing is checked — the escape hatch that lets the stdlib and the self-hosted compiler work, unsafe by design. | [docs/spec.md](../docs/spec.md), [stdlib/stdlib.ax](../stdlib/stdlib.ax) |

### 5. Diagnostics, multi-file loading, and FFI

| Term | 中文说明 | Definition (English) | Source |
|---|---|---|---|
| **`Diag` and its stage** | 统一诊断结构；stage 取 lex/parse/type/internal/link/io。 | The unified diagnostic struct `Diag { stage, file: u32, line, col, message }`. `stage` is one of `lex` / `parse` / `type` / `internal` / `link` / `io`, and `file` indexes the compilation-wide file registry (resolved to a name only when printing or when emitting `--json`, which escapes newlines and control characters). Compiler-internal failures must surface as `internal` diagnostics, never as panics (LLVM's own fatal errors excepted). | [src/lib.rs](../src/lib.rs), [src/files.rs](../src/files.rs) |
| **include-once and import cycle detection** | 每个规范化路径只包含一次；循环导入以完整链条报错。 | `load_program` resolves `import "..."` recursively, with paths relative to the **importing** file: each canonical path is included exactly once, and cycles are rejected by an import stack that reports the whole chain (`circular import: a -> b -> a`, stage `io`). The self-hosted loader implements the same semantics in `selfhost/load.ax`. | [src/lib.rs](../src/lib.rs), [selfhost/load.ax](../selfhost/load.ax) |
| **`extern def` (C FFI)** | 无函数体的 C 声明，链接期解析；检查器只强制"不能叫 `main`、不能泛型"两条。 | A bodyless C-runtime declaration resolved at link time; the checker enforces exactly two restrictions — it cannot be named `main` and it cannot be generic. The stdlib uses it for `sqrt`/`floor`/`ceil`, `malloc`/`realloc`/`free`/`memcpy`, the `fopen` family and `system`, and `fopen`/`system` take `string` parameters. **`docs/spec.md`'s claim that externs "cannot use `string` params/returns yet" is out of date**: measured, `extern def strlen(s: string) -> int` compiles and runs. Pointers cross the boundary as plain `int` (ABI-identical on x86-64). | [stdlib/stdlib.ax](../stdlib/stdlib.ax), [src/typecheck.rs](../src/typecheck.rs) |

### 6. Optimization, caching, and platforms

| Term | 中文说明 | Definition (English) | Source |
|---|---|---|---|
| **optimization levels `--O0..--O3`** | O0 无流水线 + fast-isel；O1/O2/O3 对应 default<O1/O2/O3>，默认 O3。 | `--O0` skips the IR pipeline and creates the target machine with `LLVMCodeGenLevelNone` (LLVM's fast-isel path); `--O1`/`--O2`/`--O3` select the `default<O1>`/`default<O2>`/`default<O3>` pipelines, with O3 the default (the documented "parity with `clang -O3`" promise). At most one level flag may be given, otherwise the process exits with code 2. | [src/main.rs](../src/main.rs), [src/codegen.rs](../src/codegen.rs) |
| **`AOXN_PASSES`** | 覆盖 LLVM pass 流水线文本（任意 > O0 级别）。 | An environment variable that overrides the LLVM pass-pipeline text at any level above `--O0` (for example `AOXN_PASSES=default<O1>`). It is part of the build-cache key, so changing it is a cache miss. | [src/main.rs](../src/main.rs), [docs/spec.md](../docs/spec.md) |
| **content-hash cache key** | run 与 build 共用的可执行文件缓存键：源码内容 + 编译器身份 + 影响代码生成的选项。 | The key of the executable cache shared by `aoxn run` and `aoxn build`: it covers the contents of the entry file **and every transitive import**, the compiler binary's identity (size + mtime) and every codegen-affecting option (level, `AOXN_CPU`, `AOXN_PASSES`, `-l`/`-L`, the resolved clang path). Entries live in `target/cache` (`AOXN_CACHE_DIR` relocates, `AOXN_NO_CACHE=1` disables) with a 64-entry approximate LRU; on a hit `run` executes the cached executable and `build` copies it to `-o`. | [src/main.rs](../src/main.rs), [CHANGELOG.md](../CHANGELOG.md) v0.26.2 / v0.26.3 |
| **`platform::llvm_link_name()`** | 按平台探测 LLVM C API 的 `-l` 名字。 | Probes the `-l` name of the LLVM C API per platform: `LLVM-C` on Windows/macOS, the newest `LLVM-<N>` on Debian/Ubuntu (whose C API lives in a versioned `libLLVM-<N>.so`), else `LLVM`; `AOXN_LLVM_LIB` overrides the probe. Hardcoding `-lLLVM-C` is what failed on Linux CI with `cannot find -lLLVM-C`. | [src/platform.rs](../src/platform.rs), [docs/platform-support.md](../docs/platform-support.md) |
| **`target_os()`** | 编译期内建，返回 windows/linux/macos/other，两侧实现必须一致。 | A compile-time builtin returning `"windows"` / `"linux"` / `"macos"` / `"other"`, folded into a module-private string constant — **not** a runtime syscall. Both implementations (the Rust side's `platform::target_os_name()` and `selfhost/codegen.ax`) must fold the same value or the byte-exact fixed point breaks. It is currently the language's only platform-awareness facility. | [docs/spec.md](../docs/spec.md), [src/codegen.rs](../src/codegen.rs) |
| **COFF / ELF / Mach-O** | 目标文件格式由三元组决定，走同一条发射路径。 | The object-file format is chosen by the target triple, and all three go through the same `LLVMTargetMachineEmitToFile(..., 1)` call (`1` = object, `0` = assembly). The fixed point compares COFF on Windows and would compare ELF/Mach-O elsewhere with the same logic. | [docs/platform-support.md](../docs/platform-support.md), [CONTRIBUTING.md](../CONTRIBUTING.md) |
| **PIE and `RELOC_PIC`** | 非 Windows 用 PIC（PIE），Windows 用默认重定位。 | Linux and macOS link PIE by default, so the target machine uses `RELOC_PIC` (2) off Windows and `RELOC_DEFAULT` (0) on Windows; the self-hosted counterpart is `CG_RELOC()`. | [src/llvm.rs](../src/llvm.rs), [src/codegen.rs](../src/codegen.rs), [docs/platform-support.md](../docs/platform-support.md) |
| **rpath** | POSIX 上为每个 `-L` 追加 `-Wl,-rpath,<dir>`。 | On POSIX, `link_opts` appends `-Wl,-rpath,<dir>` for every `-L` directory: an executable linked against a non-default LLVM directory (Homebrew's `libLLVM-C.dylib` only re-exports `@rpath/libLLVM.dylib`) cannot start without it. Windows linkers reject `-rpath`, so that branch is POSIX-only. | [CHANGELOG.md](../CHANGELOG.md) v0.26.2, [src/lib.rs](../src/lib.rs) |

### 7. Self-hosting (bootstrap)

| Term | 中文说明 | Definition (English) | Source |
|---|---|---|---|
| **bootstrap and stages 0/1/2** | stage 0 = Rust 编译器；stage 1 = Aoxn 写的编译器；stage 2 = 由 stage 1 编译出的编译器。 | The bootstrap ladder: stage 0 is today's Rust compiler; stage 1 is the compiler written in Aoxn; stage 2 is the compiler that stage 1 produces (`target/selfhost_stage2.exe`). Stage 3 keeps the fixed point green in CI. | [docs/selfhost.md](../docs/selfhost.md) |
| **fixed point** | 自举编译器自我编译后，IR 与目标文件与 Rust 编译器逐字节一致。 | The acceptance shape of self-hosting: the Aoxn-written driver compiles **the entire self-hosting compiler** (~7k lines: driver + codegen + typecheck + parser + lexer + loader + stdlib) into stage 2, stage 2 compiles the same program, and the two sides' **IR and object files are byte-identical** (v0.22 compared stdout/exit codes only, v0.24 added IR bytes, v0.25 added object bytes). | [CHANGELOG.md](../CHANGELOG.md) v0.22 / v0.24 / v0.25 |
| **`selfhost_driver_self_compiles`** | 承载固定点的测试；缺标准 LLVM 安装时自行跳过（当前只在 Windows 上跑）。 | The end-to-end test that carries the fixed point: it builds `selfhost/driver_self_demo.ax`, runs it to produce stage 2, has stage 2 compile `STDLIB_USE_PROG`, and finally compares IR and object bytes from both sides. It **skips itself** unless `C:/Program Files/LLVM/lib/LLVM-C.lib` exists, so the byte-exact fixed point currently runs on Windows only. | [tests/pipeline.rs](../tests/pipeline.rs) |
| **bootstrap crutch** | Rust 编译器在自举期间的角色，之后退化为测试预言机。 | The role the Rust compiler plays during the bootstrap: it must stay usable until stage 2 is stable, after which it degrades into a test oracle only. A new language feature has to land on the Rust side before the self-hosted side can port it. | [docs/selfhost.md](../docs/selfhost.md) |

### 8. Web benchmarks and performance reading

| Term | 中文说明 | Definition (English) | Source |
|---|---|---|---|
| **the `bb_*` byte-buffer helpers** | 响应渲染进可复用字节缓冲，返回新长度；CRLF 是字节 13/10。 | The byte-buffer append helpers in `web/http_buf.ax` (`bb_byte` / `bb_crlf` / `bb_str` / `bb_int`, all returning the **new** length). A long-running server must render into reusable buffers because Aoxn strings are immutable and concat results are intentionally never freed — per-request concatenation would balloon RSS. That is also why HTTP CRLF is written as raw bytes 13/10: the language has no `\r` escape. | [web/http_buf.ax](../web/http_buf.ax), [web/README.md](../web/README.md) |
| **oha** | 基准负载生成器（1.16.0，PGO），32 条 keep-alive 连接。 | The load generator used by the benchmark suite (oha 1.16.0, PGO build) with 32 keep-alive connections. autocannon is deliberately not used: a single-core client caps out near 4k req/s and masks server differences. | [docs/web-benchmark.md](../docs/web-benchmark.md), [web/README.md](../web/README.md) |
| **parity test / `clang -O3` parity** | ① web 功能对等测试；② 与 clang -O3 同性能的承诺。 | Two usages: (1) `web/loadtest/parity.mjs`, the cross-platform functional test that the Aoxn server's three route bodies are byte-identical to the Node reference plus 404, keep-alive and `/metrics`; (2) the performance promise "parity with `clang -O3`", supported by the same-algorithm comparisons in `examples/bench_*.ax`. | [web/loadtest/parity.mjs](../web/loadtest/parity.mjs), [docs/spec.md](../docs/spec.md) |
| **in-flight requests read via Little's law** | 用 req/s × 平均延迟 反推服务器是否饱和。 | Reading throughput and latency through `average in-flight requests ≈ req/s × mean latency`: Aoxn sits at about 0.7 in-flight requests out of 32 connections (idle more than 95% of the time, so the measured 7.6k req/s is the load generator's ceiling), while Node.js and Next.js sit near 28 (connections queueing). That is why latency and in-flight counts, not raw rps, tell the servers apart. | [docs/web-benchmark.md](../docs/web-benchmark.md) |

### Common confusions

1. **`//` is not a comment — it is integer division** (comments are `#` only). `x = 6 // 2` yields 3; `// comment` is a
   syntax error.
2. **The `;` in `[T; N]` is part of the array-type syntax**, not a statement terminator: Aoxn has no statement
   semicolons, and `;` appears nowhere else.
3. **"No GC" means concat results are not freed** (immutable strings, leaked by design), not that the language has no
   memory management: the stdlib's `Vec`, `buf_new` and `read_file` call `malloc`/`realloc`/`free` directly and freeing
   is the caller's job.
4. **`--O1` makes compilation faster, not the program**: it runs `default<O1>`, which on recursion/inlining-heavy code
   (fib) is about 38% slower at runtime while loop-shaped code is unaffected — which is exactly why O3 stays the default.
5. **Aoxn and Axon are the same language, old and new name** (renamed in v0.10.0). `AXON_LLVM_DIR` is a retained legacy
   alias that is still read; `AXON_DUMP_IR` is not — the source only reads `AOXN_DUMP_IR`.
6. **`README.md`'s Status section and `docs/spec.md` are stale**: the former still says "v0.7 · 70/70 tests" and the
   latter is labelled v0.9, with two further statements that contradict the implementation — its Statements section says
   `while` is the only loop (no `for` yet), although `for` has long existed, and its Program structure section says externs
   cannot use `string` parameters, although the stdlib's `fopen`/`system` do exactly that. The real baseline is v0.26.3 with
   97 end-to-end tests, and `CHANGELOG.md` is the authoritative version history.

---

## 源文件 / Source files

- [CHANGELOG.md](../CHANGELOG.md) — version history; the authority for every "when did this land" claim on this page
- [README.md](../README.md) — project overview (its Status section is stale: v0.7 / 70 tests)
- [CONTRIBUTING.md](../CONTRIBUTING.md) — `.ax` byte rules, codegen invariants, the Windows-only fixed point
- [docs/spec.md](../docs/spec.md) — layout rules, types, value semantics, builtins, tooling contract (labelled v0.9)
- [docs/selfhost.md](../docs/selfhost.md) — bootstrap stages, stage-0 crutch, the capability/gap analysis
- [docs/platform-support.md](../docs/platform-support.md) — B1–B4 / S1–S3 findings and the Tier-2 fixes
- [docs/platform-migration-plan.md](../docs/platform-migration-plan.md) — milestones M0–M5 and the `target_os()` decision
- [docs/web-platform-plan.md](../docs/web-platform-plan.md) — TS front end, independent package management, CSS decisions
- [docs/web-benchmark.md](../docs/web-benchmark.md) — oha methodology and the Little's law in-flight reading
- [src/lexer.rs](../src/lexer.rs) — layout tokens, `paren_depth`, f-string parts
- [src/parser.rs](../src/parser.rs) — the grammar as implemented; `import "path"` only
- [src/ast.rs](../src/ast.rs) — `GENERIC_LEN`
- [src/typecheck.rs](../src/typecheck.rs) — `FnSig`, `StructTable`, `call_map`, `mangle`, the monomorphizer
- [src/codegen.rs](../src/codegen.rs) — aggregate ABI, temp caches, string length caching, `nsw`/`inbounds`, opt levels
- [src/lib.rs](../src/lib.rs) — `Diag`, `load_program` include-once and cycle detection, `link_opts`
- [src/files.rs](../src/files.rs) — the diagnostic file registry
- [src/main.rs](../src/main.rs) — the CLI and the content-hash cache key
- [src/platform.rs](../src/platform.rs) — extensions, stack flag, `target_os_name`, `llvm_link_name`
- [src/llvm.rs](../src/llvm.rs) — `RELOC_PIC` and the codegen level constants
- [stdlib/stdlib.ax](../stdlib/stdlib.ax) — `Vec`, byte buffers, file IO, `system_exit_code`
- [selfhost/codegen.ax](../selfhost/codegen.ax), [selfhost/driver.ax](../selfhost/driver.ax) — `CG_RELOC()`, `target_os()` folding
- [tests/pipeline.rs](../tests/pipeline.rs) — the 97 tests, including `selfhost_driver_self_compiles`
- [web/http_buf.ax](../web/http_buf.ax), [web/loadtest/parity.mjs](../web/loadtest/parity.mjs) — `bb_*` buffers and the parity test
