# 自举：用 Aoxn 重写编译器 · Self-Hosting
> **v0.29.0 更新**：自举编译器的代码生成已从 LLVM-C 驱动重写为 C 文本发射（镜像 `codegen_c.rs`），自举固定点改为对比两侧编译器产出的**生成 C 文本与目标文件**（逐字节一致；目标文件比较会屏蔽 COFF 时间戳——clang 每次运行写入的墙钟时间，偏移 4–8 字节），且不再依赖 LLVM 库——只要 clang 存在即可在任何平台验证。本页下文关于"驱动 LLVM-C""IR 逐字节"的段落保留为历史记录。
>

> **中文**：Aoxn 编译器已经用 Aoxn 自己重写（`selfhost/*.ax`）并跑通固定点——Rust 侧构建与 Aoxn 侧构建的编译器对同一程序产出的生成 C 与目标文件逐字节一致（v0.29.0 起；目标文件比较屏蔽 COFF 时间戳）；**`docs/selfhost.md` 是停滞在数组落地之前（约 v0.19–v0.20）的历史评估稿，本页以源码与测试为准**。
> **English**: The Aoxn compiler has been rewritten in Aoxn itself (`selfhost/*.ax`) and reaches the fixed point — the Rust-built and Aoxn-built compilers emit byte-identical generated C and object files for the same program (since v0.29.0; the object comparison masks the COFF timestamp); **`docs/selfhost.md` is a historical feasibility assessment frozen before arrays landed (around v0.19–v0.20), so this page follows the source and the tests instead**.

## 中文

### 0. 先读这一句：`docs/selfhost.md` 是历史文档

`docs/selfhost.md` 是一份历史稿，停滞在数组尚未落地的时期（约 v0.19–v0.20）——它自己的 Progress 段写的是 "stages 1–3 are complete"
（lexer + parser + typecheck 含泛型单态化），stage 4 的 codegen/driver 已能发射标量与结构体、只差数组，它的进度和"剩余工作"都已经过时——
原文写 `Remaining: arrays, then broader codegen coverage`，而数组支持在 v0.21 落地、固定点在 v0.22 就跑通了。
本页的每个事实都在 v0.26.3 的工作树上核对过：`selfhost/*.ax` 源码、`tests/pipeline.rs` 里名称含
`selfhost` 的 10 个用例、`CHANGELOG.md`；行数用 `(Get-Content <file> | Measure-Object -Line).Lines`
与 `(Get-Content <file>).Count` 统计。

读源码时还会撞上几处**注释/文档落后于实现**的小漂移（不影响结论）：

| 位置 | 写法 | 代码里实际是 |
|---|---|---|
| `selfhost/typecheck.ax` 文件头（第 11–12 行） | "generic declarations/calls are reported as unsupported" | `check_all` 已完整单态化（`unify` / `mangle` / `instantiate_call`），demo 会打印实例名 `id.i` / `first.i.?.3` |
| `selfhost/codegen.ax` 文件头 | 按 slice-1 / slice-2 叙述覆盖范围 | 两者都已实现，并含嵌套聚合（2D 数组、结构体里的数组字段） |
| `selfhost/codegen.ax` 的 `Gen.loc_tag` 行内注释 | "type-arena tag of each local (TY_INT / TY_BOOL)" | 实际存的是类型 arena 的**索引**，消费处一律 `ty_tag(g.c, lty)` |
| `selfhost/driver_self_demo.ax` 注释 | "clang must be on PATH"（v0.29.0 起） | 代码里没有任何环境变量读取（语言还没有 env 设施）；不再需要 LLVM 库目录，clang 从 PATH 解析 |
| `selfhost/stdlib.ax` | 看起来像自举用的标准库 | v0.10 起的**旧副本**（git 最后一次改动 `12e4fe5`），当前没有任何文件 import 它；所有自举模块 import 的是 `../stdlib/stdlib.ax` |

### 1. 为什么可行：`examples/ffi_llvm.ax` 已经证明的事

自举最大的未知数是"能不能从 Aoxn 里驱动 LLVM"。仓库里有一个 41 行的证明：用 `extern def` 声明 12 个
LLVM-C 函数，建模块、建函数、建基本块、`LLVMBuildAdd`、`LLVMBuildRet`，最后把模块 IR 打印出来。

```aoxn
extern def LLVMContextCreate() -> int
extern def LLVMInt64TypeInContext(ctx: int) -> int
extern def LLVMFunctionType(ret: int, params: int, count: int, varargs: int) -> int
extern def LLVMBuildAdd(b: int, lhs: int, rhs: int, name: string) -> int
extern def LLVMPrintModuleToString(m: int) -> string
```

```powershell
cargo run -- run examples\ffi_llvm.ax
```

三个事实构成了自举的地基（对应 `docs/selfhost.md` 第 1 节的结论，这部分至今有效）：

1. **指针以 `int` 承载**：LLVM 的 handle 是 8 字节不透明值，Aoxn `int` 是 i64，在目标 ABI 上一致，
   持有 handle 不需要语言有指针类型；
2. **字符串就是 NUL 结尾的字节缓冲**：字符串字面量与堆字符串可以直接当 `char*` 传给 C API；
3. **`-l` / `-L` 透传给 clang**（v0.8.1 起）：`-l LLVM-C -L "C:\Program Files\LLVM\lib"` 就能把 LLVM-C 链进来。

于是整个 LLVM-C 表面都可以逐个 `extern def` 声明。`selfhost/codegen.ax` 里现在有 **85 条 `extern def`**
（LLVM-C 加 `strtod`/`strlen`/`strcmp` 这类 C 运行时），做完的动作与 Rust 侧 `src/codegen.rs` 一一对应：
建模块 → 建函数与基本块 → 发射 IR → `LLVMVerifyModule` → `LLVMRunPasses("default<O3>")` →
`LLVMTargetMachineEmitToFile`（目标文件类型 `1`）。

### 2. 组件清单（截至 v0.26.3）

行数两列：**有效行（非空行）** = `(Get-Content <file> | Measure-Object -Line).Lines`（该命令**忽略空行**）；
**物理行（含空行）** = `(Get-Content <file>).Count`。

| 文件 | 职责 | 行数（有效/物理） | 验证它的测试 | demo 入口 |
|---|---|---|---|---|
| `selfhost/lexer.ax` | 词法：Python 式布局（NEWLINE/INDENT/DEDENT）、`#` 注释、括号续行、转义、浮点、f-string 原文捕获 | 671 / 758 | `selfhost_lexer_token_stream`（逐 token 精确断言） | `lex_demo.ax` |
| `selfhost/parser.ax` | 递归下降 → arena AST（first-child + next-sibling，5 个并行 Vec）；泛型头、f-string 子解析脱糖 | 1160 / 1274 | `selfhost_parser_ast_dump`（精确 AST dump） | `parse_demo.ax` |
| `selfhost/typecheck.ax` | 严格规则 + 签名/结构体表 + 泛型单态化（`TY_VAR` 与长度 `N` 统一、AST 克隆替换、实例 mangle、去重队列） | 1428 / 1512 | `selfhost_typechecker_accepts_and_rejects`、`selfhost_frontend_handles_stdlib` | `tycheck_demo.ax` |
| `selfhost/load.ax` | 多文件 import：相对路径解析、include-once、环检测、共享 arena + `LoadState.root` | 112 / 122 | `selfhost_frontend_handles_imports` | `load_demo.ax` |
| `selfhost/codegen.ax` | LLVM-C 代码生成：标量/字符串/结构体/数组/嵌套聚合、print/len/str、原生内存内建、O3 与目标文件发射 | 1931 / 2027 | `selfhost_codegen_int_slice`（driver 系列间接覆盖） | `codegen_demo.ax` |
| `selfhost/driver.ax` | 编排 load → check → codegen → `system("clang ...")` 链接；`emit_ir` 是自举版 `aoxn ir` | 86 / 93 | `selfhost_driver_links_hello`、`selfhost_driver_compiles_stdlib` | `driver_demo.ax`、`driver_stdlib_demo.ax` |

自举编译器本体（上面六个模块 + 标准库 `stdlib/stdlib.ax`，243 / 294）合计 **约 6,080 物理行**
（有效行 5,631）。`CHANGELOG.md` 与本地会话笔记里说的 "~7k lines of Aoxn" 是同一量级的粗略说法。

九个 demo 是自举的入口，各自也是一份可运行的说明：

| demo | 用途 | 行数（有效/物理） | 需要 LLVM-C |
|---|---|---|---|
| `lex_demo.ax` | Aoxn 词法器给一段示例源码打 token 流 | 18 / 20 | 否 |
| `parse_demo.ax` | Aoxn 解析器 dump 示例 AST（含泛型、f-string、import） | 68 / 71 | 否 |
| `tycheck_demo.ax` | 接受合法程序、拒绝非法程序、打印单态化实例名 | 49 / 55 | 否 |
| `load_demo.ax` | 加载 `examples/stdlib_demo.ax`（2 文件）并整程序检查 | 22 / 23 | 否 |
| `codegen_demo.ax` | 检查 + 生成 + 发射 `selfhost_out.obj` | 33 / 34 | 是 |
| `driver_demo.ax` | 用 Aoxn 编译器把 `hello.ax` 编成可执行文件 | 12 / 13 | 是 |
| `driver_stdlib_demo.ax` | 编 `stdlib_use.ax`（导入真 stdlib）并 dump `stdlib_use.ir` | 18 / 19 | 是 |
| `driver_frontend_demo.ax` | 用 Aoxn 编译器编自己的 lexer/parser demo → `target/sh_lex`、`target/sh_parse` | 18 / 19 | 是 |
| `driver_self_demo.ax` | 固定点：用 Aoxn 编译器编整个自举编译器 → `target/selfhost_stage2` | 26 / 29 | 是 |

### 3. 编译器阶梯与固定点

```text
stage 0   Rust 编译器（src/*.rs）                  —— 唯一的拐杖：它编译 stage 1
stage 1   selfhost/*.ax（Aoxn 写的编译器）          —— 由 stage 0 编译出来
stage 2   stage 1 编译 selfhost/driver_stdlib_demo.ax —— 由 Aoxn 编译器产出的编译器
固定点    stage 1 与 stage 2 对同一程序产出逐字节相同的 IR 与目标文件
```

| 里程碑 | 版本 | 验证到什么程度 |
|---|---|---|
| 行为固定点 | v0.22 | stage-2 编出的程序 stdout + 退出码 == Rust 编译器编出的同一程序 |
| IR 层固定点 | v0.24 | 两边产出的模块 IR 文本逐字节相同（自举版 `aoxn ir`：`gen_ir_text` + `driver.emit_ir`） |
| 目标文件级固定点（Object-level，COFF 逐字节） | v0.25 | 两边产出的 COFF 目标文件也逐字节相同，失败时报告字节数与首个差异偏移 |

> 注：`CHANGELOG.md` 把 v0.24 的 IR 逐字节比对称作 **Artifact-level**（v0.25 的 COFF 比对才是 **Object-level**）；本表按实际比较对象命名。

**固定点为什么是自举的验收标准**：自举的意义是"编译器能复现自己"。行为一致只能证明被测程序上语义相同；
一旦 stage-2 的 IR 与目标文件与 stage-1 逐字节相同，就说明两边在常量、ABI、发射顺序、指令选择之前的 IR
形状上都一致——这是可机械验证、没有主观判断的强条件，也是回归防线。达成之后 Rust 编译器从"拐杖"降级成
"测试预言机"：stage-2 可以继续编译 stage-3，复现同一个编译器不再需要 Rust 参与。

`selfhost_driver_self_compiles`（`tests/pipeline.rs`）实际做的事：

1. 用 Rust 编译器把 `driver_self_demo.ax` 编成 demo 可执行文件并运行；
2. 该 demo 用 Aoxn 编译器编译 `selfhost/driver_stdlib_demo.ax`（连带 driver + codegen + typecheck +
   parser + lexer + loader + stdlib 全部源码），产出 `target/selfhost_stage2.exe`；
3. stage-2 在干净目录里编译共享 fixture `STDLIB_USE_PROG`（导入真 `stdlib/stdlib.ax`：泛型
   `sort`/`binary_search`/`sum_int`、`Vec`、原生内存、2D 数组、结构体里的数组字段、短路逻辑），
   stdout 必须是 `driver OK`；
4. 同时用 Rust 编译器编出 stage-1 的同一个 driver，跑同一个 fixture；
5. 断言：产物 stdout/退出码与 Rust 参考编译一致（`STDLIB_USE_OUT`、exit 0）；`stdlib_use.ir` 两边
   逐字节相同（先检查 IR 形状含 `define i32 @main` 与 `@aoxn.main`）；`selfhost_stdlib.obj` 两边逐字节相同。

范围要说清楚：逐字节比对的对象是**这一个共享 fixture 程序**（IR 用发射后、跑 O3 之前的模块文本），
不是"任意程序"。更广的语义覆盖由 `selfhost_codegen_int_slice`、`selfhost_driver_compiles_stdlib`、
`selfhost_driver_compiles_selfhost_frontend` 用 stdout/退出码一致性兜住。

### 4. 运行方式

前端四个 demo（lex / parse / tycheck / load）不需要额外链接参数（`aoxn` 等价于 `cargo run --`）：

```powershell
aoxn run selfhost\lex_demo.ax
aoxn run selfhost\parse_demo.ax
aoxn run selfhost\tycheck_demo.ax
aoxn run selfhost\load_demo.ax      # 从仓库根运行：它读 examples\stdlib_demo.ax
```

带 codegen / driver 的要加 LLVM-C 链接参数：

```powershell
aoxn run selfhost\codegen_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
aoxn run selfhost\driver_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
aoxn run selfhost\driver_stdlib_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
aoxn run selfhost\driver_frontend_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
aoxn run selfhost\driver_self_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
cargo run -- run selfhost\driver_self_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
```

在 Linux/macOS 上把 `-l`/`-L` 换成该平台探测出的值（Debian/Ubuntu 的 C API 在版本化
`libLLVM-<N>.so` 里，所以链接名形如 `LLVM-18`）：

```bash
# macOS (Homebrew)
cargo run -- run selfhost/driver_self_demo.ax -l LLVM-C -L /opt/homebrew/opt/llvm/lib
# Debian/Ubuntu
cargo run -- run selfhost/driver_stdlib_demo.ax -l LLVM-18 -L /usr/lib/llvm-18/lib
```

Aoxn 侧的工作目录与依赖约定（都写在 demo 的注释里）：

- `codegen_demo.ax`、`driver_demo.ax`、`driver_stdlib_demo.ax` 在**当前目录**读写产物
  （`selfhost_out.obj`、`selfhost_hello.*`、`stdlib_use.ax`/`.ir`），所以要先进一个可写目录并备好输入；
  `driver_frontend_demo.ax` 与 `driver_self_demo.ax` 固定从仓库根运行，产物落在 `target/`。
- 链接由 `selfhost/driver.ax` 用 `system("clang " + obj_path + " -o " + exe_path + ...)` 完成：Windows 上
  追加 `-Wl,/STACK:8388608`（POSIX 主线程栈本来就是 8MB），`-l`/`-L` 追加在后面。因此 **clang 必须在
  PATH 上**，并且 `LLVM-C.dll` 要在可执行文件旁边或 PATH 上（`build.rs` 会把它复制进 `target/` 的相关目录）。
- 跑全部自举用例：`cargo test --test pipeline selfhost`（名称含 `selfhost` 的 10 个测试）。

### 5. Aoxn 侧的关键工程纪律

这些都是值语义语言写编译器时被真实 bug 教出来的做法：

- **写回式风格**：`PState`/`CState`/`Gen` 按值传递，任何修改状态的函数都必须把新状态返回，调用点写成
  `p = parse_expr(p)`。历史上（v0.11）`new_node` 里写 `p.n_tag = vec_push(...)` 修改的是本地副本，
  调用方的 arena Vec 仍是 `data=0`，第一次 `vec_set` 就写 NULL 段错误。
- **多值结果走字段**：`p.res_node`（刚产生的节点）、`p.res_vec`（产生的 Vec）、`c.res_ok` + `c.r_ty`
  （表达式检查结果）、`g.rv`/`g.rk`（发射出的值与种类）。**绝不把有副作用的调用嵌成实参**
  ——每一步都先赋值再传，否则求值顺序与被丢弃的写回会一起咬人。
- **arena + 整数索引代替指针**：AST 是 `n_tag/n_sval/n_ival/n_child/n_next` 五个并行 `Vec`，
  用 first-child + next-sibling 串起来；类型也是 arena（`t_tag/t_elem/t_len/t_sname`）。没有指针、
  没有所有权、拷贝便宜，也不需要 GC。
- **全局根放在 `LoadState.root`**：`parse_program` 每解析一个文件就覆盖 `p.root`，所以拼接后的全局根
  必须存在 `LoadState` 里，`load_program` 结束时再写回 `ls.p.root`。
- **拼接前先 detach**：`append_child` 不会重置被追加节点的 `next`，所以把每个文件的顶层节点接到全局根
  之前要 `vec_set(ls.p.n_next, ch, -1)`（`selfhost/load.ax` 第 110–113 行），否则会形成自环。

### 6. 移植陷阱清单（每条都是踩过的坑）

| 陷阱 | 原因（一句话） | 位置 / 出处 |
|---|---|---|
| call 实参链要沿 ARG 节点走 | 每个实参是一个 `N_ARG` 节点，值在它的 child 上，兄弟关系在 ARG 之间；对第一个值取 `node_next` 会读到别的链 | v0.12 修复；`codegen.ax` `emit_call`、`typecheck.ax` `check_call` |
| `and`/`or`/`not` 词法成 `&&`/`\|\|`/`!` | 词法器把它们直接映射成符号 token，**没有独立关键字 token**，所以 parser/checker 只认符号 op | v0.12 修复；`lexer.ax` 第 315–320 行 |
| `extern` 的返回类型是最后一个 child | extern 没有 body block，而普通函数的返回类型是 body 前一个 child；按"body 前一个"读 extern 会报 malformed function | v0.12 修复；`typecheck.ax` `ret_node_of` |
| 类型 arena 的**索引** vs **tag** | 局部变量表存的是索引，`ty_ll`/`ty_tag` 吃索引，而 `tag_kind` 吃 tag；混用会把索引当 tag 用，静默给局部变量分配垃圾类型 | v0.15 修复；`codegen.ax` 第 222/340/420/672 行附近 |
| `bool` 是 i1 | 条件、`!`、短路 phi、`print(bool)` 都在 i1 上做，值语义拷贝按 1 字节处理 | v0.15；`ty_ll` 返回 `g.i1` |
| 没有十六进制字面量 | 语言没有 hex，所以 `_setmode(1, 0x8000)` 只能写成十进制 `32768`（`src/lexer.rs` 里没有任何 hex 扫描） | `codegen.ax` 第 1931 行 |
| `range(n)` 单参数要先搬到 `end` | 旧实现把值读进 `start` 后清零 `start`，`end` 保持 0 → 空操作数导致模块校验失败（此前只测过 `range(a, b)`） | v0.21 修复；`codegen.ax` 第 1694–1698 行 |
| `if` 合并块必须从各分支**真实 end block** 发射 | 分支体里的嵌套 compound（`elif` 会脱糖成嵌套 `if`）已经移动了 builder，用 `then_bb`/`else_bb` 分支会把合并块留成无终结指令 | v0.21 修复；`codegen.ax` 第 1621–1640 行 |
| 结构体 ABI：参数传指针 + sret | 按值把结构体写进 LLVM 函数签名会让 LLVM 挂住；现在参数是 `ptr`（callee `memcpy` 进本地槽），返回值用隐藏 sret 指针 + `ret void` | v0.19；`codegen.ax` `declare_fn`/`emit_call` |

### 7. 现状与能力边界

**已实现**（其中一部分由 demo 输出或测试断言钉住）：

- 标量 `int`/`float`/`bool`/`string`：算术、比较、一元 `-`/`!`、`print`/`len`/`str`；
- 控制流：`if`/`elif`/`else`、`while`、`for`-range（1–3 参数）、`for x in arr`、`break`/`continue`；
- 字符串：`+`、`==`/`!=`/`>` 已进 fixture；`<`/`<=`/`>=`、三参数 `range`、`load_f64`/`store_f64` 已实现但未进自举 fixture 断言（`len` 与 f-string 脱糖 `"lit" + str(expr)` 链照旧由 demo 断言钉住）；
- 结构体：具名类型、字段读写、值语义 `memcpy`、参数指针 + sret 返回；
- 数组：字面量、`[e] * N` 运行时填充、索引读写、`len`、for-in、与结构体同一套 ABI；
- 嵌套聚合：2D 数组、结构体里的数组字段、子数组实参、数组字面量直接传给数组参数；
- 泛型单态化调用（`call_node`/`call_fni` 路由）与短路 `and`/`or`（branch + phi）；
- 原生内存内建（`load_i64`/`load_f64`/`load_u8`/`store_*`/`as_ptr`/`as_string`）——自举的逃生舱。

**尚未覆盖 / 已知边界**（截至 v0.26.3，按"会影响什么"排序）：

- `main.rs` 的 CLI 语义整体没有移植（子命令、`-o`、`--json`、优化级别、构建缓存）：**语言还没有
  argv**，所以 driver 只能接受写死的路径参数，自举编译器还不是一个能替代 `aoxn` 的 CLI；
- stdlib 导入路径下的 f-string 组合没有被固定点 fixture 覆盖，浮点 `str()` 也只在 demo 覆盖到的
  格式化路径上被断言（`str(float)` 走 `snprintf("%f")`）；
- 自举侧 loader 用窄字符 `fopen`（`stdlib.read_file`），**非 ASCII 路径会失败**；Rust 侧不受影响，
  测试把 fixture 目录保持在 ASCII（`target/shdriver-*`、`target/shstdlib-*`、`target/shself-*` 等，
  测试注释里明确写了这个原因）；
- 仍然依赖外部 clang 与 LLVM-C 动态库：自举编译器没有自己的链接器，也不是自包含的。

### 8. 平台与固定点边界

- **逐字节固定点对比目前只在 Windows 上验证**：`selfhost/driver_self_demo.ax` 第 24 行把链接名硬编码成
  `"LLVM-C"`，而 `selfhost_driver_self_compiles` 开头就以 `C:/Program Files/LLVM/lib/LLVM-C.lib`
  是否存在为前提，不存在就打印 `skipping: standard LLVM install not found` 直接返回——非 Windows
  平台必然走到这条分支。
- 自举侧其余部分已经平台化（v0.26.1 / v0.26.2）：`driver.ax` 的 `-Wl,/STACK` 与 `codegen.ax` 的
  `_setmode` 都用 `target_os() == "windows"` 守卫；`CG_RELOC()` 非 Windows 返回 PIC(2)；目标后端同时
  注册 X86 与 AArch64；`driver_self_demo.ax` 的 lib dir 由 `target_os()` 选
  （Windows `C:/Program Files/LLVM/lib`、macOS `/opt/homebrew/opt/llvm/lib`、其它 `/usr/lib/llvm-18/lib`）。
- Linux/macOS 侧由**四平台 CI 矩阵**（`windows-latest` / `ubuntu-latest` / `macos-13` / `macos-14`）
  跑同一套 97 个测试来覆盖；`selfhost_*` 用例本身用平台助手（`platform::llvm_link_name()` 探测链接名、
  `std::env::join_paths` 拼 PATH、平台相关的 `EXE` 常量），所以 `selfhost_codegen_int_slice`、
  `selfhost_driver_links_hello`、`selfhost_driver_compiles_stdlib`、
  `selfhost_driver_compiles_selfhost_frontend` 在非 Windows 上会真跑，只有逐字节固定点会跳过。
  细节与 T1–T5 修复表见 [平台支持](Platform-Support.md)。
- 所以结论要说准：**"自举在 Linux/macOS 上能跑"成立，"逐字节固定点已在 Linux/macOS 验证"不成立**。

### 9. 下一步阶梯（计划，不是现状）

1. **给语言加 argv，然后移植 `main.rs` 的 CLI 语义**：`build`/`run`/`ir` 子命令、`-o`、`--json`、
   优化级别与缓存；这是"自举编译器能当编译器用"的最后一块（`docs/platform-support.md` 的遗留段把它
   记为后续阶梯）。
2. **给 `extern def` 增加链接名别名机制**：这样 `driver_self_demo.ax` 不必硬编码 `"LLVM-C"`，
   非 Windows 上才能打开逐字节固定点（同一遗留段的建议；当前只能靠给 demo 传库名，而传参又需要 argv）。
3. **扩大 codegen 覆盖**：把 stdlib 路径里的 f-string、浮点 `str()` 的其余格式化路径纳入固定点或
   parity 断言；并把 `examples/` 全量编译从"手工跑"变成套件里的断言——目前测试只用到
   `examples/stdlib_demo.ax` 与 `examples/fib.ax`，而 `CHANGELOG.md` v0.22 记录过一次
   "整个 examples 套件（11 个程序）零失败"的手工验证。

### 10. 想动手参与自举：从这里开始

1. **先看它跑起来**（不需要 LLVM-C）：`aoxn run selfhost\lex_demo.ax`、`aoxn run selfhost\parse_demo.ax`、
   `aoxn run selfhost\tycheck_demo.ax`、`aoxn run selfhost\load_demo.ax`；再跑
   `cargo test --test pipeline selfhost` 看 10 个用例的全貌。
2. **挑一个组件改**：词法改 `selfhost/lexer.ax`，语法改 `selfhost/parser.ax`，规则改
   `selfhost/typecheck.ax`，发射改 `selfhost/codegen.ax`，import 改 `selfhost/load.ax`，
   链接与编排改 `selfhost/driver.ax`；语义变更必须**同时**改 Rust 侧（`src/lexer.rs` … `src/codegen.rs`），
   否则固定点会立刻变红——这正是它作为回归防线的价值。
3. **加覆盖而不是改断言**：新能力优先做成 demo 里的真实程序 + `tests/pipeline.rs` 里对 stdout/退出码的
   精确断言；改完后固定点必须仍然绿（`cargo test --test pipeline selfhost_driver_self_compiles`）。
4. **调试配方**：生成的代码运行时崩 → 先查种子值（对空 `Vec` 调 `vec_get(v, -1)` 会解引用 NULL）；
   编译器自己崩 → `AOXN_TC_TRACE=1` / `AOXN_CG_TRACE=1` 看逐函数阶段标记；输出静默错误 → 用
   `AOXN_DUMP_IR=1` 或自举版 `emit_ir` 把两边 IR dump 出来逐字节 diff。

## English

### 0. Read this first: `docs/selfhost.md` is a historical document

`docs/selfhost.md` is a historical document that stopped before arrays landed (around v0.19–v0.20): its own
Progress note says "stages 1–3 are complete" (lexer + parser + typecheck, including generic
monomorphization) and stage 4's codegen/driver already emits scalars and structs, missing only arrays. Its
progress notes and its remaining-work list are both out of date. It still says `Remaining: arrays, then broader
codegen coverage`,
while array support landed in v0.21 and the fixed point was reached in v0.22. Everything on this page was
checked against the v0.26.3 working tree: the `selfhost/*.ax` sources, the 10 tests in `tests/pipeline.rs`
whose names contain `selfhost`, and `CHANGELOG.md`. Line counts come from
`(Get-Content <file> | Measure-Object -Line).Lines` and `(Get-Content <file>).Count`.

A few comments in the sources also lag behind the implementation (they do not change any conclusion):

| Location | What it says | What the code does |
|---|---|---|
| `selfhost/typecheck.ax` header (lines 11–12) | "generic declarations/calls are reported as unsupported" | `check_all` performs full monomorphization (`unify` / `mangle` / `instantiate_call`); the demo prints instance names `id.i` / `first.i.?.3` |
| `selfhost/codegen.ax` header | describes coverage as slice-1 / slice-2 | both are implemented, including nested aggregates (2D arrays, struct fields holding arrays) |
| `Gen.loc_tag` inline comment in `codegen.ax` | "type-arena tag of each local (TY_INT / TY_BOOL)" | it actually stores a type-arena **index**; every consumer calls `ty_tag(g.c, lty)` |
| `selfhost/driver_self_demo.ax` line 6 | "AOXN_LLVM_LIBDIR overrides the lib dir" | no environment variable is read anywhere (the language has no env facility); the lib dir is a three-way `target_os()` branch |
| `selfhost/stdlib.ax` | looks like the self-hosting standard library | a **stale copy** from v0.10 (last touched in `12e4fe5`), imported by nothing; every selfhost module imports `../stdlib/stdlib.ax` |

### 1. Why this is feasible: what `examples/ffi_llvm.ax` already proved

The biggest unknown in self-hosting was whether LLVM can be driven from Aoxn at all. The repository ships a
41-line proof: 12 LLVM-C functions declared with `extern def`, a module, a function, a basic block,
`LLVMBuildAdd`, `LLVMBuildRet`, and the module IR printed out.

```aoxn
extern def LLVMContextCreate() -> int
extern def LLVMInt64TypeInContext(ctx: int) -> int
extern def LLVMFunctionType(ret: int, params: int, count: int, varargs: int) -> int
extern def LLVMBuildAdd(b: int, lhs: int, rhs: int, name: string) -> int
extern def LLVMPrintModuleToString(m: int) -> string
```

```powershell
cargo run -- run examples\ffi_llvm.ax
```

Three facts form the foundation (they are the still-valid part of `docs/selfhost.md` §1):

1. **Pointers travel as `int`**: LLVM handles are 8-byte opaque values and Aoxn `int` is i64, ABI-identical
   on the target, so holding a handle needs no pointer type in the language.
2. **Strings are NUL-terminated byte buffers**: string literals and heap strings can be passed straight to
   the C API as `char*`.
3. **`-l` / `-L` are forwarded to clang** (since v0.8.1): `-l LLVM-C -L "C:\Program Files\LLVM\lib"` is enough
   to link LLVM-C.

That makes the whole LLVM-C surface reachable one `extern def` at a time. `selfhost/codegen.ax` now carries
**85 `extern def`s** (LLVM-C plus C runtime helpers such as `strtod`/`strlen`/`strcmp`) and performs exactly
what `src/codegen.rs` does: create the module → declare functions and basic blocks → emit IR →
`LLVMVerifyModule` → `LLVMRunPasses("default<O3>")` → `LLVMTargetMachineEmitToFile` (object file type `1`).

### 2. Component inventory (as of v0.26.3)

Two line counts: **non-blank lines** = `(Get-Content <file> | Measure-Object -Line).Lines` (that cmdlet
**ignores blank lines**); **physical lines** = `(Get-Content <file>).Count`.

| File | Responsibility | Lines (non-blank/physical) | Test that verifies it | Demo entry point |
|---|---|---|---|---|
| `selfhost/lexer.ax` | Lexing: Python-style layout (NEWLINE/INDENT/DEDENT), `#` comments, paren continuation, escapes, floats, raw f-string capture | 671 / 758 | `selfhost_lexer_token_stream` (exact token stream) | `lex_demo.ax` |
| `selfhost/parser.ax` | Recursive descent → arena AST (first-child + next-sibling, 5 parallel Vecs); generic headers, f-string desugaring by sub-parsing | 1160 / 1274 | `selfhost_parser_ast_dump` (exact AST dump) | `parse_demo.ax` |
| `selfhost/typecheck.ax` | Strict rules + signature/struct tables + generic monomorphization (`TY_VAR` and length `N` unification, AST clone/substitution, instance mangling, dedup queue) | 1428 / 1512 | `selfhost_typechecker_accepts_and_rejects`, `selfhost_frontend_handles_stdlib` | `tycheck_demo.ax` |
| `selfhost/load.ax` | Multi-file imports: relative resolution, include-once, cycle rejection, shared arena + `LoadState.root` | 112 / 122 | `selfhost_frontend_handles_imports` | `load_demo.ax` |
| `selfhost/codegen.ax` | LLVM-C code generation: scalars/strings/structs/arrays/nested aggregates, print/len/str, raw memory builtins, O3 and object emission | 1931 / 2027 | `selfhost_codegen_int_slice` (plus the driver tests indirectly) | `codegen_demo.ax` |
| `selfhost/driver.ax` | Orchestrates load → check → codegen → `system("clang ...")` link; `emit_ir` is the self-hosted `aoxn ir` | 86 / 93 | `selfhost_driver_links_hello`, `selfhost_driver_compiles_stdlib` | `driver_demo.ax`, `driver_stdlib_demo.ax` |

The self-hosting compiler itself (those six modules plus `stdlib/stdlib.ax`, 243 / 294) is **about 6,080
physical lines** (5,631 code lines). The "~7k lines of Aoxn" figure in `CHANGELOG.md` and in the local
session notes is a rounded version of the same magnitude.

The nine demos are the entry points, and each one doubles as runnable documentation:

| Demo | Purpose | Lines (non-blank/physical) | Needs LLVM-C |
|---|---|---|---|
| `lex_demo.ax` | The Aoxn lexer tokenizes a sample program | 18 / 20 | no |
| `parse_demo.ax` | The Aoxn parser dumps a sample AST (generics, f-string, import) | 68 / 71 | no |
| `tycheck_demo.ax` | Accepts a valid program, rejects an invalid one, prints instance names | 49 / 55 | no |
| `load_demo.ax` | Loads `examples/stdlib_demo.ax` (2 files) and checks the merged program | 22 / 23 | no |
| `codegen_demo.ax` | Checks, generates and emits `selfhost_out.obj` | 33 / 34 | yes |
| `driver_demo.ax` | The Aoxn compiler compiles `hello.ax` into an executable | 12 / 13 | yes |
| `driver_stdlib_demo.ax` | Compiles `stdlib_use.ax` (imports the real stdlib) and dumps `stdlib_use.ir` | 18 / 19 | yes |
| `driver_frontend_demo.ax` | The Aoxn compiler compiles its own lexer/parser demos → `target/sh_lex`, `target/sh_parse` | 18 / 19 | yes |
| `driver_self_demo.ax` | Fixed point: the Aoxn compiler compiles the whole self-hosting compiler → `target/selfhost_stage2` | 26 / 29 | yes |

### 3. The compiler ladder and the fixed point

```text
stage 0   the Rust compiler (src/*.rs)                 -- the only crutch: it builds stage 1
stage 1   selfhost/*.ax (the compiler written in Aoxn) -- produced by stage 0
stage 2   stage 1 compiles selfhost/driver_stdlib_demo.ax -- a compiler produced by the Aoxn compiler
fixed point  stage 1 and stage 2 emit byte-identical IR and object files for the same program
```

| Milestone | Version | How far it is verified |
|---|---|---|
| Behavioral fixed point | v0.22 | stdout + exit code of a program built by stage-2 match the Rust compiler's build |
| IR-level fixed point | v0.24 | the module IR text from both sides is byte-identical (self-hosted `aoxn ir`: `gen_ir_text` + `driver.emit_ir`) |
| Object-file-level fixed point (Object-level, byte-identical COFF) | v0.25 | the emitted COFF objects are byte-identical too; a mismatch reports both sizes and the first differing offset |

> Note: `CHANGELOG.md` labels the v0.24 IR-byte comparison **Artifact-level** (the v0.25 COFF comparison is **Object-level**); these rows are named by what is actually compared.

**Why the fixed point is the acceptance criterion for self-hosting**: self-hosting means "the compiler can
reproduce itself". Equal behavior only proves equal semantics on the program under test; once stage-2's IR
and object file are byte-identical to stage-1's, the two agree on constants, ABI, emission order and the IR
shape before instruction selection — a mechanically checkable condition with no room for judgement, and a
regression fence. Once it holds, the Rust compiler is demoted from "crutch" to "test oracle": stage-2 can
build stage-3, reproducing the same compiler without Rust in the loop.

What `selfhost_driver_self_compiles` (in `tests/pipeline.rs`) actually does:

1. The Rust compiler builds `driver_self_demo.ax` into a demo executable, which is then run.
2. That demo uses the Aoxn compiler to compile `selfhost/driver_stdlib_demo.ax` (pulling in driver +
   codegen + typecheck + parser + lexer + loader + stdlib), producing `target/selfhost_stage2.exe`.
3. Stage-2 compiles the shared fixture `STDLIB_USE_PROG` in a clean directory (it imports the real
   `stdlib/stdlib.ax`: generic `sort`/`binary_search`/`sum_int`, `Vec`, raw memory, 2D arrays, structs with
   array fields, short-circuit logic); stdout must be `driver OK`.
4. In parallel, the Rust compiler builds the same driver (stage 1) and runs it on the same fixture.
5. Assertions: the product's stdout/exit code match the Rust reference (`STDLIB_USE_OUT`, exit 0);
   `stdlib_use.ir` is byte-identical on both sides (after checking the IR shape contains `define i32 @main`
   and `@aoxn.main`); `selfhost_stdlib.obj` is byte-identical on both sides.

State the scope precisely: the byte-exact comparison covers **this one shared fixture program** (the IR is
the post-emission module text, before the O3 pipeline runs), not "any program". Wider semantics are pinned
by `selfhost_codegen_int_slice`, `selfhost_driver_compiles_stdlib` and
`selfhost_driver_compiles_selfhost_frontend` through stdout/exit-code parity.

### 4. How to run it

The four front-end demos (lex / parse / tycheck / load) need no extra link flags (`aoxn` is equivalent to
`cargo run --`):

```powershell
aoxn run selfhost\lex_demo.ax
aoxn run selfhost\parse_demo.ax
aoxn run selfhost\tycheck_demo.ax
aoxn run selfhost\load_demo.ax      # run from the repo root: it reads examples\stdlib_demo.ax
```

Codegen / driver demos need the LLVM-C link flags:

```powershell
aoxn run selfhost\codegen_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
aoxn run selfhost\driver_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
aoxn run selfhost\driver_stdlib_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
aoxn run selfhost\driver_frontend_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
aoxn run selfhost\driver_self_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
cargo run -- run selfhost\driver_self_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
```

On Linux/macOS, substitute the values probed for that platform (Debian/Ubuntu keep the C API inside a
versioned `libLLVM-<N>.so`, so the link name looks like `LLVM-18`):

```bash
# macOS (Homebrew)
cargo run -- run selfhost/driver_self_demo.ax -l LLVM-C -L /opt/homebrew/opt/llvm/lib
# Debian/Ubuntu
cargo run -- run selfhost/driver_stdlib_demo.ax -l LLVM-18 -L /usr/lib/llvm-18/lib
```

Working-directory and dependency conventions on the Aoxn side (all documented in the demo headers):

- `codegen_demo.ax`, `driver_demo.ax` and `driver_stdlib_demo.ax` read and write artifacts in the
  **current directory** (`selfhost_out.obj`, `selfhost_hello.*`, `stdlib_use.ax`/`.ir`), so `cd` into a
  writable directory and provide the input first. `driver_frontend_demo.ax` and `driver_self_demo.ax`
  always run from the repo root and write into `target/`.
- Linking is done by `selfhost/driver.ax` through `system("clang " + obj_path + " -o " + exe_path + ...)`:
  Windows appends `-Wl,/STACK:8388608` (POSIX main-thread stacks are 8MB already) and `-l`/`-L` are appended
  after it. So **clang must be on PATH**, and `LLVM-C.dll` must sit next to the executable or on PATH
  (`build.rs` copies it into the relevant directories under `target/`).
- To run every self-hosting test: `cargo test --test pipeline selfhost` (the 10 tests whose names contain
  `selfhost`).

### 5. Key engineering discipline on the Aoxn side

Every rule below was learned from a real bug in a value-semantics language:

- **Write-back style**: `PState`/`CState`/`Gen` are passed by value, so any function that mutates state must
  return the new state and every call site writes `p = parse_expr(p)`. In v0.11 `new_node` wrote
  `p.n_tag = vec_push(...)` into a local copy, the caller's arena Vecs stayed at `data=0`, and the first
  `vec_set` stored through NULL — a segfault.
- **Multi-value results travel through fields**: `p.res_node` (the node just produced), `p.res_vec` (a
  produced Vec), `c.res_ok` + `c.r_ty` (expression checking result), `g.rv`/`g.rk` (emitted value and kind).
  **Never nest a side-effecting call as an argument** — assign each step, or evaluation order and discarded
  write-backs will bite together.
- **Arena + integer indices instead of pointers**: the AST is five parallel `Vec`s
  (`n_tag/n_sval/n_ival/n_child/n_next`) linked first-child + next-sibling; types are an arena too
  (`t_tag/t_elem/t_len/t_sname`). No pointers, no ownership, cheap copies, no GC.
- **The global root lives in `LoadState.root`**: `parse_program` overwrites `p.root` for every file, so the
  spliced program root must be kept in `LoadState` and written back to `ls.p.root` by `load_program`.
- **Detach before splicing**: `append_child` does not reset the appended node's `next`, so before attaching
  each file's top-level nodes to the global root you must `vec_set(ls.p.n_next, ch, -1)`
  (`selfhost/load.ax` lines 110–113), otherwise you create a self-cycle.

### 6. Porting traps (every one of them was hit for real)

| Trap | Why (one sentence) | Location / origin |
|---|---|---|
| Call-argument chains must walk ARG nodes | each argument is an `N_ARG` node with its value as a child, and siblings are linked between ARGs; taking `node_next` of the first value reads a different chain | fixed in v0.12; `emit_call` in `codegen.ax`, `check_call` in `typecheck.ax` |
| `and`/`or`/`not` lex as `&&`/`\|\|`/`!` | the lexer maps them straight to the symbolic tokens, so there are **no separate keyword tokens** and the parser/checker only know the symbolic ops | fixed in v0.12; `lexer.ax` lines 315–320 |
| An `extern`'s return type is the last child | externs have no body block, while a normal function's return type sits before the body; reading "the child before the body" reports an extern as a malformed function | fixed in v0.12; `ret_node_of` in `typecheck.ax` |
| Type-arena **index** vs **tag** | the locals table stores indices, `ty_ll`/`ty_tag` take indices and `tag_kind` takes tags; mixing them silently allocates garbage element types for locals | fixed in v0.15; `codegen.ax` around lines 222/340/420/672 |
| `bool` is i1 | conditions, `!`, short-circuit phis and `print(bool)` all operate on i1, and value-semantics copies treat it as one byte | v0.15; `ty_ll` returns `g.i1` |
| There are no hex literals | the language has no hex, so `_setmode(1, 0x8000)` has to be written in decimal as `32768` (there is no hex scanning anywhere in `src/lexer.rs`) | `codegen.ax` line 1931 |
| `range(n)` with one argument must move the value to `end` | the old code read it into `start` and then zeroed `start`, leaving `end` null → module verification failure (only `range(a, b)` had ever been tested) | fixed in v0.21; `codegen.ax` lines 1694–1698 |
| `if` merge branches must be emitted from each branch's **real end block** | a nested compound inside a branch (`elif` desugars to a nested `if`) has already moved the builder, so branching from `then_bb`/`else_bb` leaves the merge block unterminated | fixed in v0.21; `codegen.ax` lines 1621–1640 |
| Struct ABI: pointer parameters + sret | by-value struct types in LLVM signatures hung LLVM; parameters are now `ptr` (the callee `memcpy`s into its local slot) and returns use a hidden sret pointer with `ret void` | v0.19; `declare_fn`/`emit_call` in `codegen.ax` |

### 7. Current state and capability boundary

**Implemented** (part of it pinned by demo outputs or test assertions):

- scalars `int`/`float`/`bool`/`string`: arithmetic, comparisons, unary `-`/`!`, `print`/`len`/`str`;
- control flow: `if`/`elif`/`else`, `while`, `for`-range (1–3 arguments), `for x in arr`, `break`/`continue`;
- strings: `+`, `==`/`!=`/`>` are in the fixture; `<`/`<=`/`>=`, three-argument `range` and `load_f64`/`store_f64` are implemented but not asserted by the self-hosting fixture (`len` and f-string desugaring into `"lit" + str(expr)` chains are still pinned by the demos);
- structs: named types, field read/write, value-semantics `memcpy`, pointer parameters + sret returns;
- arrays: literals, `[e] * N` runtime fill, index read/write, `len`, for-in, the same ABI as structs;
- nested aggregates: 2D arrays, arrays inside struct fields, sub-array arguments, array literals passed
  straight to array parameters;
- monomorphized generic calls (`call_node`/`call_fni` routing) and short-circuit `and`/`or` (branch + phi);
- raw memory builtins (`load_i64`/`load_f64`/`load_u8`/`store_*`/`as_ptr`/`as_string`) — the self-hosting
  escape hatch.

**Not covered / known boundaries** (as of v0.26.3, ordered by impact):

- `main.rs` CLI semantics are not ported at all (subcommands, `-o`, `--json`, optimization levels, build
  cache): **the language still has no argv**, so the driver only accepts hardcoded paths and the
  self-hosted compiler is not yet a drop-in `aoxn` CLI;
- f-strings on stdlib import paths are not covered by the fixed-point fixture, and floating-point `str()`
  is only asserted on the formatting paths the demos exercise (`str(float)` uses `snprintf("%f")`);
- the self-hosted loader opens files with narrow `fopen` (`stdlib.read_file`), so **non-ASCII paths fail**;
  the Rust compiler is unaffected and the tests keep fixture directories ASCII (`target/shdriver-*`,
  `target/shstdlib-*`, `target/shself-*`, with the reason stated in the test comments);
- an external clang and the LLVM-C dynamic library are still required: the self-hosted compiler has no
  linker of its own and is not self-contained.

### 8. Platform and fixed-point boundary

- **The byte-exact fixed point is currently verified on Windows only**: `selfhost/driver_self_demo.ax`
  line 24 hardcodes the link name `"LLVM-C"`, and `selfhost_driver_self_compiles` starts by requiring
  `C:/Program Files/LLVM/lib/LLVM-C.lib` to exist — otherwise it prints
  `skipping: standard LLVM install not found` and returns, which is exactly what happens off Windows.
- The rest of the self-hosting side is already platformized (v0.26.1 / v0.26.2): `-Wl,/STACK` in
  `driver.ax` and `_setmode` in `codegen.ax` are gated on `target_os() == "windows"`; `CG_RELOC()` returns
  PIC(2) off Windows; both the X86 and AArch64 backends are registered; `driver_self_demo.ax` picks the
  lib dir from `target_os()` (Windows `C:/Program Files/LLVM/lib`, macOS `/opt/homebrew/opt/llvm/lib`,
  otherwise `/usr/lib/llvm-18/lib`).
- Linux/macOS are covered by the **four-platform CI matrix** (`windows-latest` / `ubuntu-latest` /
  `macos-13` / `macos-14`) running the same 97 tests; the `selfhost_*` tests use platform helpers
  (`platform::llvm_link_name()` probing, `std::env::join_paths` for PATH, the platform-aware `EXE`
  constant), so `selfhost_codegen_int_slice`, `selfhost_driver_links_hello`,
  `selfhost_driver_compiles_stdlib` and `selfhost_driver_compiles_selfhost_frontend` really do run off
  Windows — only the byte-exact fixed point is skipped. Details and the T1–T5 fix table are in
  [Platform Support](Platform-Support.md).
- So be precise about the conclusion: **"self-hosting works on Linux/macOS" holds; "the byte-exact fixed
  point has been verified on Linux/macOS" does not**.

### 9. Next rungs (plans, not current state)

1. **Add argv to the language, then port `main.rs` CLI semantics**: the `build`/`run`/`ir` subcommands,
   `-o`, `--json`, optimization levels and the cache. This is the last piece for "the self-hosted compiler
   can be used as a compiler" (recorded as a follow-up rung in `docs/platform-support.md`).
2. **Give `extern def` a link-name alias mechanism**: then `driver_self_demo.ax` need not hardcode
   `"LLVM-C"` and the byte-exact fixed point can be enabled off Windows (the same follow-up note; today it
   would need a library name passed to the demo, and parameter passing needs argv).
3. **Widen codegen coverage**: bring f-strings on stdlib paths and the remaining floating-point `str()`
   formatting paths into the fixed point or parity assertions, and turn the full `examples/` compile from a
   manual run into suite assertions — the tests currently reference only `examples/stdlib_demo.ax` and
   `examples/fib.ax`, while `CHANGELOG.md` v0.22 records a manual run of the whole examples suite
   (11 programs) with zero failures.

### 10. Want to work on self-hosting? Start here

1. **Watch it run first** (no LLVM-C needed): `aoxn run selfhost\lex_demo.ax`,
   `aoxn run selfhost\parse_demo.ax`, `aoxn run selfhost\tycheck_demo.ax`,
   `aoxn run selfhost\load_demo.ax`; then `cargo test --test pipeline selfhost` to see all 10 tests.
2. **Pick a component**: lexing → `selfhost/lexer.ax`, grammar → `selfhost/parser.ax`, rules →
   `selfhost/typecheck.ax`, emission → `selfhost/codegen.ax`, imports → `selfhost/load.ax`, linking and
   orchestration → `selfhost/driver.ax`. Every semantic change must land on the Rust side too
   (`src/lexer.rs` … `src/codegen.rs`), otherwise the fixed point turns red immediately — which is exactly
   what makes it a regression fence.
3. **Add coverage instead of editing assertions**: express new capabilities as a real program in a demo
   plus an exact stdout/exit-code assertion in `tests/pipeline.rs`, and make sure the fixed point is still
   green (`cargo test --test pipeline selfhost_driver_self_compiles`).
4. **Debugging recipe**: a crash in generated code → check seed values first (`vec_get(v, -1)` on an empty
   `Vec` dereferences NULL); a crash in the compiler → `AOXN_TC_TRACE=1` / `AOXN_CG_TRACE=1` for per-function
   stage markers; silently wrong output → dump both IRs with `AOXN_DUMP_IR=1` or the self-hosted
   `emit_ir` and diff them byte for byte.

---

## 源文件 / Source files

- [selfhost/lexer.ax](../selfhost/lexer.ax) — Aoxn lexer (token kinds, `keyword_kind`, `lex_all`)
- [selfhost/parser.ax](../selfhost/parser.ax) — arena AST (`PState`: `n_tag/n_sval/n_ival/n_child/n_next`)
- [selfhost/typecheck.ax](../selfhost/typecheck.ax) — `CState`, `ret_node_of`, `check_call`, `unify`/`mangle`/`instantiate_call`
- [selfhost/load.ax](../selfhost/load.ax) — `load_file`/`load_program`, `LoadState.root`, detach-before-splice
- [selfhost/codegen.ax](../selfhost/codegen.ax) — 85 `extern def`s, `ty_ll`, `declare_fn`, `emit_str`, `emit_for`, `emit_if`, `gen_program`, `gen_ir_text`, `gen_emit_object`
- [selfhost/driver.ax](../selfhost/driver.ax) — `compile_file_libs` (clang `system()` link), `emit_ir`
- [selfhost/lex_demo.ax](../selfhost/lex_demo.ax), [selfhost/parse_demo.ax](../selfhost/parse_demo.ax), [selfhost/tycheck_demo.ax](../selfhost/tycheck_demo.ax), [selfhost/load_demo.ax](../selfhost/load_demo.ax) — front-end demos
- [selfhost/codegen_demo.ax](../selfhost/codegen_demo.ax), [selfhost/driver_demo.ax](../selfhost/driver_demo.ax), [selfhost/driver_stdlib_demo.ax](../selfhost/driver_stdlib_demo.ax), [selfhost/driver_frontend_demo.ax](../selfhost/driver_frontend_demo.ax), [selfhost/driver_self_demo.ax](../selfhost/driver_self_demo.ax) — codegen and driver demos
- [tests/pipeline.rs](../tests/pipeline.rs) — the 10 `selfhost_*` tests, `STDLIB_USE_PROG`/`STDLIB_USE_OUT`, `llvm_link_name()`, `path_with_llvm_bin()`
- [stdlib/stdlib.ax](../stdlib/stdlib.ax) — `Vec`, `sort`/`binary_search`, `read_file` (narrow `fopen`), `system`/`system_exit_code`
- [examples/ffi_llvm.ax](../examples/ffi_llvm.ax) — the LLVM-C feasibility proof of concept
- [docs/selfhost.md](../docs/selfhost.md) — historical feasibility assessment (superseded by this page)
- [docs/platform-support.md](../docs/platform-support.md) — platform survey, §7 T1–T5 fixes, the fixed-point remainder
- [CHANGELOG.md](../CHANGELOG.md) — v0.11–v0.26.3 entries quoted above
- `AGENTS.md` — gitignored local session notes, used only for cross-checking (not linked as repository documentation)
