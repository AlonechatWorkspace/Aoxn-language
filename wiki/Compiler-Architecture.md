# 编译器架构 · Compiler Architecture

> **中文**：Aoxn 编译器（v0.26.3）从 `.ax` 源码到本机可执行文件的端到端流水线：模块职责与规模、
> `src/lib.rs` 的对外 API 与诊断模型、多文件 `import` 加载，以及零依赖 / 手写 LLVM-C FFI 等设计约束。
> **English**: The end-to-end pipeline of the Aoxn compiler (v0.26.3) from `.ax` source to a native
> executable: module responsibilities and sizes, the `src/lib.rs` public API and diagnostic model,
> multi-file `import` loading, and design constraints such as zero dependencies and hand-written
> LLVM-C FFI.

## 中文

### 1. 端到端流水线

Aoxn 编译器只有一个进程、一次编译：源码文本一路走到目标文件，最后由 clang 链接成可执行文件。
下面的图给出数据流与负责每一段的模块。

```text
 examples/hello.ax
        │   src/lib.rs: load_file() 读文件 → files::register() → 递归解析 import
        ▼
 ┌────────────────────┐
 │ src/lexer.rs       │  lex(src, file_id) -> Vec<Token>
 │ 词法               │  Token { tok, pos }，pos = { line, col, file }
 └────────────────────┘  输出 Newline / Indent / Dedent（Python 式布局）
        ▼
 ┌────────────────────┐
 │ src/parser.rs      │  parse(tokens) -> Program（定义在 src/ast.rs）
 │ 语法               │  递归下降，`: ` + Newline Indent stmt* Dedent
 └────────────────────┘
        ▼
 ┌────────────────────┐
 │ src/typecheck.rs   │  check(&Program) -> CheckOutput
 │ 类型检查           │  严格规则 + FnSig 表 + 泛型单态化
 └────────────────────┘  instances: Vec<FnDecl> · call_map: HashMap<usize, String>
        ▼
 ┌────────────────────┐
 │ src/codegen.rs     │  generate_to_object(&program, obj, opt_level, &call_map)
 │ 代码生成           │  LLVM-C：build_module → verify → passes → isel
 └────────────────────┘  → hello.obj / hello.o
        ▼
 ┌────────────────────┐
 │ src/lib.rs         │  link_opts(obj, exe, libs, lib_paths)
 │ 链接               │  clang <obj> -o <exe> [-Wl,/STACK:8388608] [-L…] [-l…]
 └────────────────────┘  → 本机可执行文件
```

各模块的职责与大致规模（行数为 v0.26.3 时的原始行数，含空行、取整到十位，仅用于判断量级）：

| 模块 | 职责 | 约行数 |
|---|---|---|
| [src/main.rs](../src/main.rs) | CLI（`build` / `run` / `ir`）、`-o` `-l` `-L` `--json` `--cpu` `--O*`、构建缓存 | ≈470 |
| [src/lib.rs](../src/lib.rs) | 流水线编排、`Diag` 诊断、import 加载、clang 链接 | ≈520 |
| [src/files.rs](../src/files.rs) | 编译期文件名注册表（thread_local，诊断用索引换名字） | ≈35 |
| [src/lexer.rs](../src/lexer.rs) | 词法：token + 行列 + 文件编号，产出 Newline/Indent/Dedent | ≈700 |
| [src/parser.rs](../src/parser.rs) | 递归下降语法分析 → AST | ≈730 |
| [src/ast.rs](../src/ast.rs) | AST 节点、`Type`、`Pos` 定义 | ≈225 |
| [src/typecheck.rs](../src/typecheck.rs) | 严格类型规则、`FnSig` 表、泛型单态化 | ≈1180 |
| [src/codegen.rs](../src/codegen.rs) | LLVM-C 代码生成 → 目标文件 | ≈1900 |
| [src/llvm.rs](../src/llvm.rs) | 手写 LLVM-C FFI（零外部 crate） | ≈175 |
| [src/hashing.rs](../src/hashing.rs) | 内部哈希表与缓存键使用的快速 hasher | ≈40 |
| [src/platform.rs](../src/platform.rs) | 平台抽象（扩展名、栈链接旗标、LLVM 库名探测） | ≈195 |
| [tests/pipeline.rs](../tests/pipeline.rs) | 端到端测试：97 个 `#[test]`，编译 → 运行 → 断言输出 | ≈2300 |

词法与语法的细节（缩进栈、f-string 切分、语法产生式）见
[词法与语法](Frontend-Lexer-and-Parser.md)；类型规则见 [类型检查](Type-Checker.md)；LLVM 发射细节见
[代码生成与 LLVM](Codegen-and-LLVM-FFI.md)。

### 2. 各阶段做了什么

#### 2.1 词法

`lexer::lex(src: &str, file_id: u32) -> Result<Vec<Token>, Diag>` 是纯函数式的：一次扫描产出完整
token 向量，出错立刻返回一个 `stage: "lex"` 的 `Diag`。每个 token 带 `Pos { line, col, file }`，
`file` 是本次编译的文件注册表索引。缩进、空行、括号内续行、f-string 的处理都在这里完成。

#### 2.2 语法

`parser::parse(tokens: Vec<Token>) -> Result<Program, Diag>` 是递归下降解析器，只有单 token 前瞻
（外加 `peek2()`）。它把布局 token 还原成块结构，处理 `if/elif/else`、`while`、`for`、`def`（含
`def f[T, N](...)` 泛型头）、`struct`、`extern def`、`import`，并在 `if/elif/else` 处把 `elif` 折成
嵌套 `if`。f-string 也在这一层脱糖为 `"lit" + str(expr) + ...` 的 `+` 链。

#### 2.3 类型检查

`typecheck::check(&Program) -> Result<CheckOutput, Diag>` 是唯一会拒绝程序的语义阶段，顺序为：

```text
 collect_structs  收集 struct 布局（StructTable：字段顺序 + 名字到下标索引）
 pass 1           具体函数的签名（FnSig），先建表所以支持互相递归
 pass 2           泛型声明进 generics 表（此时不检查函数体）；随后校验 main 存在
 pass 3           逐个体检查具体函数体
 pass 4           排空单态化实例队列（实例体内可能再产生新实例）
```

`CheckOutput` 里有两样东西进入代码生成：`instances: Vec<FnDecl>`（单态化后的具体实例）和
`call_map: HashMap<usize, String>`（泛型调用点 AST 节点地址 → 单态化后的函数名）。`src/lib.rs` 的
`finish_to_object` / `finish_to_ir` 负责接线：

```text
 typecheck → program.funcs.retain(|f| f.type_params.is_empty())   # 泛型声明不进代码生成
           → program.funcs.extend(out.instances)                  # 追加单态化实例
           → codegen::generate_to_object(&program, obj, level, &out.call_map)
```

泛型声明永远不会到达 codegen：它的数组长度是 `GENERIC_LEN` 哨兵（`usize::MAX`），交给 LLVM 会得到
一个 40 亿元素的数组类型。

#### 2.4 代码生成

`codegen::generate_to_object(program, obj_path, opt_level, call_map)` 用 LLVM-C 建模块、跑优化管线、
发射目标文件；`codegen::generate_ir_text(program, opt_level, call_map)` 是同一流程的 `aoxn ir` 版本
（用 `LLVMPrintModuleToString` 拿文本）。`build_module` 内部的顺序是固定的：

```text
 cg.target   取默认 triple、注册 target（X86 + AArch64，Once）、建 TargetMachine、
             把 data layout 装进模块（必须在任何 IR 之前，否则聚合体大小会退化成 ptrtoint 常量表达式）
 cg.build    先发射 C 入口包装 main（用户的 main 改名为 aoxn.main）→ 结构体类型（两阶段，
             允许前向引用）→ 一次性声明全部用户函数（各自建好 entry 块，支持互相递归）→
             逐个发射函数体
 cg.verify   LLVMVerifyModule；失败变成内部错误字符串
 cg.passes   pipeline_for(opt_level)：O1/O2/O3 = default<O1>/<O2>/<O3>，O0 完全跳过；
             AOXN_PASSES 可在任何 level > 0 覆盖管线文本
 cg.isel     LLVMTargetMachineEmitToFile → .obj（Windows）/ .o（其它平台）
```

`opt_level` 同时决定 IR 管线与后端 `CodeGenOptLevel`（O0 用 None，才能真正走到 fast-isel 快路径）。

#### 2.5 链接

`link_opts(obj_path, exe_path, libs, lib_paths)` 用 `find_clang()` 找到 clang，拼出
`clang <obj> -o <exe>`，然后按平台追加 `platform::stack_link_flag()`（Windows 的
`-Wl,/STACK:8388608`）、每个 `-L<dir>`（POSIX 上再补 `-Wl,-rpath,<dir>`）与每个 `-l<name>`，最后
`status()` 等待。`build_*` 系列在链接成功后删掉临时目标文件。找不到 clang 时给出 `stage: "link"` 的
诊断而不是 panic。

### 3. `src/lib.rs` 的对外 API

编译器的库面就是流水线的拼装面。命名约定有两层：`compile_*` 只到目标文件 / IR，`build_*` 一直做到
可执行文件；带 `_lvl` 的版本收 `opt_level: u8`（`0` = O0 … `3` = O3），不带的版本收旧的 `opt: bool`
并用 `codegen::level_of` 折算（`true` = O3，`false` = O0）。另外 `*_paths_*` 版本会解析 import，
`*_sources_*` / 单字符串版本不会。

| 分组 | 函数 |
|---|---|
| 到目标文件 | `compile_to_object[_lvl]`、`compile_sources_to_object[_lvl]`、`compile_paths_to_object[_lvl]` |
| 到 LLVM IR 文本 | `compile_to_ir`、`compile_sources_to_ir[_lvl]`、`compile_paths_to_ir[_lvl]` |
| 到可执行文件 | `build_exe[_lvl]`、`build_sources_exe[_lvl]`、`build_paths_exe`、`build_paths_opts[_lvl]` |
| 只做链接 | `link`、`link_opts` |
| 工具 | `dependency_files`、`find_clang`、`diags_to_json`、`diag_to_string` |

CLI 实际用到的正是这几个：`main.rs` 调 `build_paths_opts_lvl`（`run` / `build`）、
`compile_paths_to_ir_lvl`（`ir`）、`dependency_files` + `hashing::FastBuild`（构建缓存键）、
`find_clang`（缓存键的一部分）、`diags_to_json` / `diag_to_string`（报错）与 `platform::exe_ext`
（默认输出名）。

公开模块与其中值得注意的入口：

| 模块 | 公开入口 |
|---|---|
| `ast` | `Program`、`StructDecl`、`FnDecl`、`Stmt`、`Expr`、`Type`、`Pos`、`GENERIC_LEN` |
| `lexer` | `lex`、`Tok`、`Token`、`FStrPart` |
| `parser` | `parse` |
| `typecheck` | `check`、`CheckOutput`、`FnSig`、`StructInfo`、`StructTable` |
| `codegen` | `generate_to_object[_opt]`、`generate_ir_text[_opt]`、`level_of` |
| `files` | `register`、`name`、`clear` |
| `hashing` | `FastBuild`、`FastHasher` |
| `platform` | `exe_ext`、`obj_ext`、`stack_link_flag`、`is_windows` / `is_linux` / `is_macos`、`llvm_dir_candidates`、`target_os_name`、`llvm_link_name`、`llvm_link_name_in` |

一个最小的库调用长这样（Rust 片段）：

```text
 use aoxn::{build_paths_exe, diag_to_string};

 let paths = vec!["examples/hello.ax".to_string()];
 match build_paths_exe(&paths, std::path::Path::new("hello.exe"), true) {
     Ok(()) => {}
     Err(diags) => {
         for d in &diags {
             eprintln!("{}", diag_to_string(d));
         }
         std::process::exit(1);
     }
 }
```

### 4. 诊断模型

整条流水线只有一种错误类型：

```text
 pub struct Diag {
     pub stage: &'static str, // "lex" | "parse" | "type" | "internal" | "link" | "io"
     pub file: u32,           // 本次编译文件注册表的索引（u32::MAX = 没有源码位置）
     pub line: usize,
     pub col: usize,
     pub message: String,
 }
```

- `Diag::at(stage, file, line, col, message)` 是有位置信息的通用构造器；`internal()` 构造
  `file: u32::MAX, line: 0, col: 0` 的编译器内部错误。流水线里唯一的“内部错误”通道是 codegen：
  它返回 `Result<(), String>`，`lib.rs` 把字符串包成 `Diag::internal`。
- `src/files.rs` 是线程局部的文件名注册表：`register(name) -> u32` 追加并返回下标，`name(idx)` 取回
  名字（越界返回 `"?"`），`clear()` 在每次编译开始时重置。诊断里只存索引，人类可读形式与 JSON 形式
  才去换名字——这也是为什么 `Diag` 可以廉价地 `Clone` 并跨阶段传递。
- `diag_to_string` 的一行格式是 `[<stage>] <file>:<line>:<col>: <message>`。`line == 0` 时不打印行号
  列号：文件名能解析出来就退化成 `[<stage>] <file>: <message>`，文件名也解析不出来（索引越界 →
  `"?"`）时整个位置前缀省略——内部、链接与部分 IO 错误就是这种形态。
- `diags_to_json` 输出 `{"ok":false,"errors":[…]}`，每个元素是
  `{"stage":…,"file":…,"line":…,"col":…,"message":…}`，字符串按 JSON 规则转义（`\`、`"`、`\n`、
  `\r`、`\t`，以及 `< 0x20` 的 `\uXXXX`）。`--json` 让 CLI 把它打到 stderr。

实测的两个例子（路径做了简化，其余原样）：

```text
 [lex] bad.ax:3:1: unindent does not match any outer indentation level (expected one of [0], found 3)
 [parse] slash.ax:2:13: expected an expression, found Slash
```

同一条缩进错误的 JSON 形态：

```text
 {"ok":false,"errors":[{"stage":"lex","file":"bad.ax","line":3,"col":1,
  "message":"unindent does not match any outer indentation level (expected one of [0], found 3)"}]}
```

### 5. 多文件加载

文件型入口（`compile_paths_*` / `build_paths_*` / `build_paths_opts*`）走 `load_program`，它把所有
入口文件与它们的传递 import 合并成**一个** AST（同一命名空间）：

```text
 load_program(entries)
   ├─ files::clear()                      # 清空文件名注册表
   ├─ LoadState { visited: HashSet<PathBuf>, stack: Vec<PathBuf>, structs, funcs }
   └─ 对每个入口 load_file(path, state)

 load_file(path, state)
   ├─ 1. std::fs::canonicalize(path)      # 失败 → stage "io"：cannot open '…'
   ├─ 2. state.stack 里已有该 canonical 路径 → "circular import: a -> b -> a"
   ├─ 3. state.visited 里已有 → 直接返回（include-once）
   ├─ 4. visited.insert + stack.push
   ├─ 5. read_to_string(path)             # 失败 → stage "io"：cannot read '…'
   ├─ 6. files::register(path.display()) → file_id；lex + parse
   ├─ 7. 对 program.imports：resolve_import(canonical.parent(), imp.path) 后递归
   │        （相对**导入者**所在目录解析；绝对路径原样使用）
   │        子调用的诊断若没有位置（file == u32::MAX、line == 0），补上 import 语句本身的位置
   ├─ 8. state.structs / state.funcs 追加本文件声明
   └─ 9. stack.pop()
```

`resolve_import` 的规则很简单：`Path::is_absolute()` 为真就用原路径，否则 `导入者目录.join(path)`。
去重与查环都基于 `canonicalize` 的结果，所以 `./a.ax` 与 `a.ax` 是同一个文件，菱形依赖只加载一次
（`import_transitive_and_include_once`、`import_cycle_detected`、`import_missing_file`、
`import_error_reports_importing_file` 覆盖这些路径）。

字符串型入口是一个刻意的例外：`parse_sources` 只接受源码文本，遇到任何 import 直接报错，因为字符串
没有“导入者目录”可以解析：

```text
 [io] <source>:1:1: import "somewhere.ax" requires compiling from files (imports resolve relative to the importing file)
```

（`string_sources_reject_imports` 测试。）另一个文件型入口是 `dependency_files(entries)`：它只做
逐行扫描（`scan_imports`）而不解析，用来给 `aoxn run` / `aoxn build` 的内容哈希缓存收集依赖；
任何一个文件读不到就返回 `None`，本次调用不使用缓存。逐行扫描对合法程序是精确的，对畸形输入最多
多算几个依赖（只降低命中率，不会产生过期命中）。

### 6. 设计约束

- **零外部 crate。** `Cargo.toml` 没有 `[dependencies]`，`Cargo.lock` 里只有 `aoxn` 自己。
- **手写 LLVM-C FFI。** `src/llvm.rs` 是对 LLVM-C 的薄声明层（`extern "C"`），不引入 `inkwell` /
  `llvm-sys`——它们需要完整的静态 LLVM 库，而官方 Windows 安装包只带 `LLVM-C.lib` + `LLVM-C.dll`。
  加新能力 = 先在 `src/llvm.rs` 加声明，并确认符号真的存在于已安装的库里。
- **编译器内部失败必须是 `internal` 诊断，不是 panic**（LLVM 自身的 fatal error 除外）。codegen 全部
  返回 `Result<_, String>`，`lib.rs` 负责转成 `Diag::internal`。
- **LLVM 的进程级全局状态要一次性初始化。** target 注册用 `static TARGET_INIT: Once`，重复注册会让
  `LLVMGetTargetFromTriple` 以 “Cannot choose between targets” 失败。
- **模块 data layout 必须先于任何 IR。** 否则 `LLVMSizeOf` 会退化成 `ptrtoint(gep)` 常量表达式，成千
  上万个这样的常量会拖垮 instcombine 与指令选择。
- **只有具体函数进入代码生成**（见 §2.3），泛型声明只存在于 typecheck 的表里。
- 平台差异集中在 `src/platform.rs`，而不是散落的 `cfg!(windows)`。

### 7. 编译期开关与调试通道

完整的环境变量表由 [命令行与工具链](CLI-and-Tooling.md) 维护，这里只列与流水线阶段直接对应的几个：

| 开关 | 作用 |
|---|---|
| `AOXN_TIME` | 分阶段墙钟时间打到 stderr |
| `AOXN_TC_TRACE` | 类型检查的逐函数标记 `[tc] <name>` |
| `AOXN_CG_TRACE` | 代码生成的逐函数标记 `[cg] <name>` |
| `AOXN_DUMP_IR` | 在 verify 之前把整个模块的 IR 打到 stderr |
| `AOXN_PASSES` | 覆盖 LLVM 管线文本（任何 level > 0） |
| `AOXN_CPU` | 目标 CPU（默认空字符串 = 通用 CPU，保证输出可复现） |

`AOXN_TIME=1` 的输出分两层。`src/lib.rs` 的 `timed()` 给出顶层阶段：

```text
 [time] lex: 123.4µs
 [time] parse: 45.6µs
 [time] typecheck: 210.9µs
 [time] codegen: 120.5ms
 [time] link: 150.2ms
```

注意文件型流水线里标签会带上文件路径（`lex <path>`、`parse <path>`），每个 import 文件各计时一次。
`src/codegen.rs` 的 `CgPhase` 再补代码生成内部的子阶段，形式是
`[time]   cg.<子阶段>   <毫秒>ms`（数值右对齐 9 位、两位小数）：

```text
 [time]   cg.target       1.20ms
 [time]   cg.build        0.80ms
 [time]   cg.verify       0.31ms
 [time]   cg.passes     110.00ms
 [time]   cg.isel         8.40ms
```

（上面的数字只是量级示意；本机 10–50ms 区间的测量抖动可达 ±2 倍，做对比时要交错多次取中位数。）

### 8. `src/hashing.rs` 与 `src/platform.rs` 的位置

`hashing.rs` 提供 FxHash 风格的 `FastBuild` / `FastHasher`（一个乘加混合的 `write`，一个 `finish` 直接
返回状态）。它服务两类调用方：编译期内部查表（typecheck 的 `SigMap`/`GenMap`/`Scopes`/`StructTable`，
codegen 的 `locals`/`fns`/`struct_fields`）与 `main.rs` 的构建缓存键（`FastBuild.build_hasher()` 哈希
入口文件 + 全部传递 import 的内容、编译器自身、以及所有影响代码生成的选项）。这些表的迭代顺序从不
进入程序输出，所以不担心哈希质量被对手利用。

`platform.rs` 是 v0.26.1 引入的平台抽象层，被 `lib.rs`、`main.rs`、`codegen.rs` 与 `build.rs` 共用：

- `exe_ext()` / `obj_ext()`：可执行文件与目标文件扩展名（Windows `.exe`/`.obj`，其它 `.o`）。
- `stack_link_flag()`：只有 Windows 追加 `-Wl,/STACK:8388608`（Windows 主线程默认 1MB 栈，而大数组是
  alloca 在栈上的；Linux 由 `RLIMIT_STACK` 决定、macOS 固定 8MB，都不需要链接旗标）。
- `is_windows()` / `is_linux()` / `is_macos()`：代替散落的 `cfg!`。
- `llvm_dir_candidates()`：`build.rs` 探测 LLVM 安装目录的顺序表。
- `llvm_link_name()` / `llvm_link_name_in()`：探测 LLVM C API 的 `-l` 名（专用 `LLVM-C`，否则最新的
  `libLLVM-<N>`，否则无版本的 `libLLVM`；`AOXN_LLVM_LIB` 可覆盖）——Debian/Ubuntu 把 C API 放在
  版本化的 `libLLVM-18.so` 里，硬编码 `LLVM-C` 在那边会链接失败。
- `target_os_name()`：`target_os()` 内建函数的编译期折叠值（`"windows" | "linux" | "macos" |
  "other"`），Rust 编译器与自举编译器必须折叠出同一个值，否则固定点会破。

### 9. 旧文档里已经过时的说法

以源码与测试为准，以下几处仓库文档落后于 v0.26.3：

- `docs/spec.md` 的标题仍写 v0.9；`## Statements` 一节仍称 `while` 是唯一的循环（“no `for` yet”），
  而 `src/parser.rs` 早已支持 `for ... in` 与 `break`/`continue`；`## Tooling contract` 里的 JSON 诊断
  形状漏了 `file` 字段，实现始终会带上它。
- `README.md` 的 Status 段仍写 v0.7 / 70 tests（Cargo.toml 是 v0.26.3，`tests/pipeline.rs` 有 97 个
  `#[test]`）。
- `docs/selfhost.md` 仍是早期可行性评估的口径（“stages 1–3 are underway”），实际上自举已经达到固定点，
  见 [自举](Self-Hosting.md)。

### 10. 继续阅读

- 词法与语法细节 → [词法与语法](Frontend-Lexer-and-Parser.md)
- 类型规则与单态化 → [类型检查](Type-Checker.md)
- LLVM 发射与 FFI → [代码生成与 LLVM](Codegen-and-LLVM-FFI.md)
- 语言规则总表 → [语言参考](Language-Reference.md)
- 命令行、优化级别与构建缓存 → [命令行与工具链](CLI-and-Tooling.md)
- 测试组织与 CI → [测试与 CI](Testing-and-CI.md)

## English

### 1. The end-to-end pipeline

The compiler is one process and one pass: source text goes all the way to an object file, and clang
performs the final link into an executable. The diagram below shows the data flow and the module
responsible for each segment.

```text
 examples/hello.ax
        │   src/lib.rs: load_file() reads the file → files::register() → resolves imports
        ▼
 ┌────────────────────┐
 │ src/lexer.rs       │  lex(src, file_id) -> Vec<Token>
 │ lexer              │  Token { tok, pos }, pos = { line, col, file }
 └────────────────────┘  emits Newline / Indent / Dedent (Python-style layout)
        ▼
 ┌────────────────────┐
 │ src/parser.rs      │  parse(tokens) -> Program (defined in src/ast.rs)
 │ parser             │  recursive descent: `:` Newline Indent stmt* Dedent
 └────────────────────┘
        ▼
 ┌────────────────────┐
 │ src/typecheck.rs   │  check(&Program) -> CheckOutput
 │ type checker       │  strict rules + FnSig table + generic monomorphization
 └────────────────────┘  instances: Vec<FnDecl> · call_map: HashMap<usize, String>
        ▼
 ┌────────────────────┐
 │ src/codegen.rs     │  generate_to_object(&program, obj, opt_level, &call_map)
 │ code generator     │  LLVM-C: build_module → verify → passes → isel
 └────────────────────┘  → hello.obj / hello.o
        ▼
 ┌────────────────────┐
 │ src/lib.rs         │  link_opts(obj, exe, libs, lib_paths)
 │ linking            │  clang <obj> -o <exe> [-Wl,/STACK:8388608] [-L…] [-l…]
 └────────────────────┘  → native executable
```

Responsibilities and approximate size of each module (figures are raw line counts at v0.26.3,
blank lines included, rounded to the nearest ten — they are there to convey scale):

| Module | Responsibility | ~Lines |
|---|---|---|
| [src/main.rs](../src/main.rs) | CLI (`build` / `run` / `ir`), `-o` `-l` `-L` `--json` `--cpu` `--O*`, build cache | ≈470 |
| [src/lib.rs](../src/lib.rs) | pipeline orchestration, `Diag` diagnostics, import loading, clang linking | ≈520 |
| [src/files.rs](../src/files.rs) | compile-wide file-name registry (thread_local; diagnostics store an index) | ≈35 |
| [src/lexer.rs](../src/lexer.rs) | tokens with line/col/file; emits Newline/Indent/Dedent | ≈700 |
| [src/parser.rs](../src/parser.rs) | recursive-descent parsing → AST | ≈730 |
| [src/ast.rs](../src/ast.rs) | AST nodes, `Type`, `Pos` | ≈225 |
| [src/typecheck.rs](../src/typecheck.rs) | strict type rules, `FnSig` table, generic monomorphization | ≈1180 |
| [src/codegen.rs](../src/codegen.rs) | LLVM-C code generation → object file | ≈1900 |
| [src/llvm.rs](../src/llvm.rs) | hand-written LLVM-C FFI (zero external crates) | ≈175 |
| [src/hashing.rs](../src/hashing.rs) | fast hasher for internal tables and the cache key | ≈40 |
| [src/platform.rs](../src/platform.rs) | platform abstraction (extensions, stack link flag, LLVM lib probing) | ≈195 |
| [tests/pipeline.rs](../tests/pipeline.rs) | end-to-end suite: 97 `#[test]`s, compile → run → assert output | ≈2300 |

Lexer and parser details (the indent stack, f-string splitting, grammar productions) live in
[Lexer and Parser](Frontend-Lexer-and-Parser.md); the type rules live in
[Type Checker](Type-Checker.md); LLVM emission lives in
[Codegen and LLVM](Codegen-and-LLVM-FFI.md).

### 2. What each stage does

#### 2.1 Lexing

`lexer::lex(src: &str, file_id: u32) -> Result<Vec<Token>, Diag>` is a single scan that produces the
whole token vector; on the first error it returns a `Diag` with `stage: "lex"`. Every token carries
`Pos { line, col, file }`, where `file` indexes this compilation's file registry. Indentation, blank
lines, bracket continuation and f-strings are all handled here.

#### 2.2 Parsing

`parser::parse(tokens: Vec<Token>) -> Result<Program, Diag>` is a recursive-descent parser with
one-token lookahead (plus `peek2()`). It turns layout tokens back into blocks and handles
`if/elif/else`, `while`, `for`, `def` (including the generic header `def f[T, N](...)`), `struct`,
`extern def` and `import`, folding `elif` into nested `if`s. f-strings are desugared at this stage
into `"lit" + str(expr) + ...` chains.

#### 2.3 Type checking

`typecheck::check(&Program) -> Result<CheckOutput, Diag>` is the only semantic stage that can reject a
program. Its order is:

```text
 collect_structs  collect struct layouts (StructTable: ordered fields + name-to-index map)
 pass 1           signatures of concrete functions (FnSig) — built first, so mutual recursion works
 pass 2           generic declarations go into the generics table (bodies are not checked yet);
                  `main` is then verified to exist
 pass 3           check every concrete function body
 pass 4           drain the monomorphization queue (instance bodies may enqueue more instances)
```

Two things leave `CheckOutput` for code generation: `instances: Vec<FnDecl>` (the monomorphized
concrete instances) and `call_map: HashMap<usize, String>` (generic call-site AST node address →
mangled instance name). `finish_to_object` / `finish_to_ir` in `src/lib.rs` wire them up:

```text
 typecheck → program.funcs.retain(|f| f.type_params.is_empty())   # generics never reach codegen
           → program.funcs.extend(out.instances)                  # append monomorphized instances
           → codegen::generate_to_object(&program, obj, level, &out.call_map)
```

A generic declaration never reaches codegen: its array length is the `GENERIC_LEN` sentinel
(`usize::MAX`), and handing that to LLVM would produce an array type with four billion elements.

#### 2.4 Code generation

`codegen::generate_to_object(program, obj_path, opt_level, call_map)` builds a module through the
LLVM-C API, runs the optimization pipeline and emits an object file.
`codegen::generate_ir_text(program, opt_level, call_map)` is the same flow for `aoxn ir` (the text
comes from `LLVMPrintModuleToString`). The order inside `build_module` is fixed:

```text
 cg.target   default triple, target registration (X86 + AArch64, Once), TargetMachine,
             and the module data layout — before any IR, or aggregate sizes degrade into
             ptrtoint constant expressions
 cg.build    C entry wrapper `main` first (the user's main becomes aoxn.main) → struct types in two
             phases (so they may forward-reference each other) → declare every user function up front
             (each with its entry block, so mutual recursion works) → emit the bodies one by one
 cg.verify   LLVMVerifyModule; a failure becomes an internal error string
 cg.passes   pipeline_for(opt_level): O1/O2/O3 → default<O1>/<O2>/<O3>, O0 skips it entirely;
             AOXN_PASSES overrides the pipeline text at any level > 0
 cg.isel     LLVMTargetMachineEmitToFile → .obj (Windows) / .o (elsewhere)
```

`opt_level` selects both the IR pipeline and the backend `CodeGenOptLevel` (O0 uses None, which is
what actually reaches LLVM's fast-isel path).

#### 2.5 Linking

`link_opts(obj_path, exe_path, libs, lib_paths)` resolves clang through `find_clang()`, builds
`clang <obj> -o <exe>`, appends `platform::stack_link_flag()` (Windows: `-Wl,/STACK:8388608`), every
`-L<dir>` (plus `-Wl,-rpath,<dir>` on POSIX) and every `-l<name>`, then waits on `status()`. The
`build_*` entry points delete the temporary object file after a successful link. When clang cannot be
found the result is a `stage: "link"` diagnostic, not a panic.

### 3. The public API of `src/lib.rs`

The library surface is the pipeline assembly surface. Naming follows two axes: `compile_*` stops at an
object file or IR text, `build_*` goes all the way to an executable; `_lvl` variants take
`opt_level: u8` (`0` = O0 … `3` = O3) while the others take the legacy `opt: bool` and route it
through `codegen::level_of` (`true` = O3, `false` = O0). Independently, `*_paths_*` entry points
resolve imports, while `*_sources_*` / single-string entry points do not.

| Group | Functions |
|---|---|
| To an object file | `compile_to_object[_lvl]`, `compile_sources_to_object[_lvl]`, `compile_paths_to_object[_lvl]` |
| To LLVM IR text | `compile_to_ir`, `compile_sources_to_ir[_lvl]`, `compile_paths_to_ir[_lvl]` |
| To an executable | `build_exe[_lvl]`, `build_sources_exe[_lvl]`, `build_paths_exe`, `build_paths_opts[_lvl]` |
| Linking only | `link`, `link_opts` |
| Helpers | `dependency_files`, `find_clang`, `diags_to_json`, `diag_to_string` |

The CLI uses exactly these: `main.rs` calls `build_paths_opts_lvl` (`run` / `build`),
`compile_paths_to_ir_lvl` (`ir`), `dependency_files` + `hashing::FastBuild` (the cache key),
`find_clang` (also part of the cache key), `diags_to_json` / `diag_to_string` (reporting) and
`platform::exe_ext` (default output name).

Public modules and the entry points worth knowing:

| Module | Public surface |
|---|---|
| `ast` | `Program`, `StructDecl`, `FnDecl`, `Stmt`, `Expr`, `Type`, `Pos`, `GENERIC_LEN` |
| `lexer` | `lex`, `Tok`, `Token`, `FStrPart` |
| `parser` | `parse` |
| `typecheck` | `check`, `CheckOutput`, `FnSig`, `StructInfo`, `StructTable` |
| `codegen` | `generate_to_object[_opt]`, `generate_ir_text[_opt]`, `level_of` |
| `files` | `register`, `name`, `clear` |
| `hashing` | `FastBuild`, `FastHasher` |
| `platform` | `exe_ext`, `obj_ext`, `stack_link_flag`, `is_windows` / `is_linux` / `is_macos`, `llvm_dir_candidates`, `target_os_name`, `llvm_link_name`, `llvm_link_name_in` |

A minimal library call looks like this (Rust fragment):

```text
 use aoxn::{build_paths_exe, diag_to_string};

 let paths = vec!["examples/hello.ax".to_string()];
 match build_paths_exe(&paths, std::path::Path::new("hello.exe"), true) {
     Ok(()) => {}
     Err(diags) => {
         for d in &diags {
             eprintln!("{}", diag_to_string(d));
         }
         std::process::exit(1);
     }
 }
```

### 4. The diagnostic model

The whole pipeline shares a single error type:

```text
 pub struct Diag {
     pub stage: &'static str, // "lex" | "parse" | "type" | "internal" | "link" | "io"
     pub file: u32,           // index into this compilation's file registry (u32::MAX = no location)
     pub line: usize,
     pub col: usize,
     pub message: String,
 }
```

- `Diag::at(stage, file, line, col, message)` is the general positioned constructor; `internal()`
  builds a `file: u32::MAX, line: 0, col: 0` compiler-internal error. The one internal-error channel in
  the pipeline is codegen: it returns `Result<(), String>` and `lib.rs` wraps the string in
  `Diag::internal`.
- `src/files.rs` is a thread-local file-name registry: `register(name) -> u32` appends and returns the
  index, `name(idx)` reads it back (`"?"` when out of range), `clear()` resets it at the start of every
  compilation. Diagnostics store only the index and resolve it when printing or serializing — which is
  why a `Diag` can be cheaply cloned and passed across stage boundaries.
- `diag_to_string` produces one line, `[<stage>] <file>:<line>:<col>: <message>`. When `line == 0` the
  line/column part is dropped: if the file name resolves, the form degrades to
  `[<stage>] <file>: <message>`; if it does not resolve either (out-of-range index → `"?"`), the whole
  location prefix is omitted — that is the shape of internal, link and some IO errors.
- `diags_to_json` emits `{"ok":false,"errors":[…]}`, with each element shaped as
  `{"stage":…,"file":…,"line":…,"col":…,"message":…}` and JSON-escaped strings (`\`, `"`, `\n`, `\r`,
  `\t`, and `\uXXXX` for control characters below `0x20`). `--json` makes the CLI print it to stderr.

Two observed examples (paths shortened, everything else verbatim):

```text
 [lex] bad.ax:3:1: unindent does not match any outer indentation level (expected one of [0], found 3)
 [parse] slash.ax:2:13: expected an expression, found Slash
```

The same indentation error as JSON:

```text
 {"ok":false,"errors":[{"stage":"lex","file":"bad.ax","line":3,"col":1,
  "message":"unindent does not match any outer indentation level (expected one of [0], found 3)"}]}
```

### 5. Multi-file loading

The path-based entry points (`compile_paths_*`, `build_paths_*`, `build_paths_opts*`) go through
`load_program`, which merges every entry file and its transitive imports into **one** AST (a single
namespace):

```text
 load_program(entries)
   ├─ files::clear()                      # reset the file-name registry
   ├─ LoadState { visited: HashSet<PathBuf>, stack: Vec<PathBuf>, structs, funcs }
   └─ load_file(path, state) for every entry

 load_file(path, state)
   ├─ 1. std::fs::canonicalize(path)      # failure → stage "io": cannot open '…'
   ├─ 2. canonical path already on state.stack → "circular import: a -> b -> a"
   ├─ 3. canonical path already visited → return (include-once)
   ├─ 4. visited.insert + stack.push
   ├─ 5. read_to_string(path)             # failure → stage "io": cannot read '…'
   ├─ 6. files::register(path.display()) → file_id; lex + parse
   ├─ 7. for each program.imports: resolve_import(canonical.parent(), imp.path), then recurse
   │        (resolved relative to the directory of the *importing* file; absolute paths as-is)
   │        a child diagnostic without a location (file == u32::MAX, line == 0) inherits the
   │        position of the import statement itself
   ├─ 8. append this file's structs / funcs to state
   └─ 9. stack.pop()
```

`resolve_import` is deliberately small: an absolute path is used as-is, otherwise it is
`importing dir.join(path)`. Deduplication and cycle detection both work on the `canonicalize` result,
so `./a.ax` and `a.ax` are the same file and a diamond dependency is loaded once
(`import_transitive_and_include_once`, `import_cycle_detected`, `import_missing_file`,
`import_error_reports_importing_file` cover these paths).

The string-based entry point is a deliberate exception: `parse_sources` only accepts source text and
rejects any import outright, because a string has no "importing directory" to resolve against:

```text
 [io] <source>:1:1: import "somewhere.ax" requires compiling from files (imports resolve relative to the importing file)
```

(`string_sources_reject_imports`.) A second path-based helper, `dependency_files(entries)`, collects
dependencies for the content-hash cache of `aoxn run` / `aoxn build` by scanning lines
(`scan_imports`) instead of parsing; it returns `None` as soon as a file cannot be read, which disables
the cache for that invocation. The line-based scan is exact for well-formed programs and may only
*over*-include on malformed input (fewer cache hits, never a stale hit).

### 6. Design constraints

- **Zero external crates.** `Cargo.toml` has no `[dependencies]` section and `Cargo.lock` contains only
  `aoxn` itself.
- **Hand-written LLVM-C FFI.** `src/llvm.rs` is a thin `extern "C"` declaration layer; `inkwell` /
  `llvm-sys` are not used because they need the full static LLVM libraries while the official Windows
  installer ships only `LLVM-C.lib` + `LLVM-C.dll`. Adding a new LLVM capability means adding the
  declaration *and* proving the symbol exists in the installed library first.
- **Compiler-internal failures surface as `internal` diagnostics, never panics** (LLVM's own fatal
  errors excepted). Codegen returns `Result<_, String>` everywhere; `lib.rs` converts the string into a
  `Diag::internal`.
- **LLVM's process-global state is initialized exactly once.** Target registration goes through
  `static TARGET_INIT: Once`; registering twice makes `LLVMGetTargetFromTriple` fail with
  "Cannot choose between targets".
- **The module data layout must be established before any IR.** Otherwise `LLVMSizeOf` degrades into a
  `ptrtoint(gep)` constant expression, and thousands of those dominate instcombine and instruction
  selection.
- **Only concrete functions reach codegen** (see §2.3 above); generic declarations live only in the
  type checker's tables.
- **Platform differences are centralized in `src/platform.rs`** instead of scattered `cfg!(windows)`
  branches.

### 7. Compile-time switches and debug channels

The complete environment-variable table is maintained in [CLI and Tooling](CLI-and-Tooling.md); the
ones that map directly onto pipeline stages are:

| Switch | Effect |
|---|---|
| `AOXN_TIME` | per-stage wall-clock timings to stderr |
| `AOXN_TC_TRACE` | per-function typechecker markers `[tc] <name>` |
| `AOXN_CG_TRACE` | per-function codegen markers `[cg] <name>` |
| `AOXN_DUMP_IR` | dump the whole module's IR to stderr before verification |
| `AOXN_PASSES` | override the LLVM pipeline text (any level > 0) |
| `AOXN_CPU` | target CPU (empty default = generic CPU, keeping output reproducible) |

`AOXN_TIME=1` prints two layers. `timed()` in `src/lib.rs` covers the top-level stages:

```text
 [time] lex: 123.4µs
 [time] parse: 45.6µs
 [time] typecheck: 210.9µs
 [time] codegen: 120.5ms
 [time] link: 150.2ms
```

Note that on the file-based path the labels carry a file path (`lex <path>`, `parse <path>`), timing
each imported file separately. `CgPhase` in `src/codegen.rs` adds the codegen sub-phases as
`[time]   cg.<sub-phase>   <milliseconds>ms` (value right-aligned in nine columns, two decimals):

```text
 [time]   cg.target       1.20ms
 [time]   cg.build        0.80ms
 [time]   cg.verify       0.31ms
 [time]   cg.passes     110.00ms
 [time]   cg.isel         8.40ms
```

(The numbers above only convey magnitude; on this dev machine measurements in the 10–50ms range swing
by up to 2×, so comparisons need interleaved runs and medians.)

### 8. Where `src/hashing.rs` and `src/platform.rs` sit

`hashing.rs` provides an FxHash-style `FastBuild` / `FastHasher` (a multiply-mix `write`, and a
`finish` that simply returns the state). It serves two kinds of caller: internal compile-time lookup
tables (typecheck's `SigMap`/`GenMap`/`Scopes`/`StructTable`, codegen's
`locals`/`fns`/`struct_fields`) and the build-cache key in `main.rs` (`FastBuild.build_hasher()` hashes
the entry file plus all transitive imports, the compiler binary itself, and every codegen-affecting
option). The iteration order of those tables never reaches program output, so adversarial hash quality
is not a concern.

`platform.rs` is the platform abstraction introduced in v0.26.1 and shared by `lib.rs`, `main.rs`,
`codegen.rs` and `build.rs`:

- `exe_ext()` / `obj_ext()`: executable and object-file extensions (Windows `.exe`/`.obj`, elsewhere
  `.o`).
- `stack_link_flag()`: Windows only, `-Wl,/STACK:8388608` (the Windows main thread has a 1MB stack
  while large arrays are alloca'd on the stack; Linux follows `RLIMIT_STACK` and macOS is fixed at
  8MB, so neither needs a linker flag).
- `is_windows()` / `is_linux()` / `is_macos()`: replaces scattered `cfg!` checks.
- `llvm_dir_candidates()`: the install-directory probe order used by `build.rs`.
- `llvm_link_name()` / `llvm_link_name_in()`: probes the `-l` name of the LLVM C API (dedicated
  `LLVM-C`, else the newest `libLLVM-<N>`, else unversioned `libLLVM`; `AOXN_LLVM_LIB` overrides) —
  Debian/Ubuntu ship the C API inside a versioned `libLLVM-18.so`, where a hardcoded `LLVM-C` fails to
  link.
- `target_os_name()`: the compile-time fold for the `target_os()` builtin (`"windows" | "linux" |
  "macos" | "other"`). The Rust compiler and the self-hosted compiler must fold the same value, or the
  fixed point breaks.

### 9. Statements in older docs that are now stale

Judged against the source and the tests, these repo docs lag behind v0.26.3:

- `docs/spec.md` still titles itself v0.9; its `## Statements` section still calls `while` the only loop
  ("no `for` yet") although `src/parser.rs` has supported `for ... in` plus `break`/`continue` for a
  long time; and the JSON diagnostic shape in `## Tooling contract` omits the `file` field that the
  implementation always emits.
- `README.md`'s Status section still says v0.7 / 70 tests (Cargo.toml says v0.26.3 and
  `tests/pipeline.rs` holds 97 `#[test]`s).
- `docs/selfhost.md` still reads as the early feasibility assessment ("stages 1–3 are underway"),
  whereas self-hosting has reached the fixed point — see [Self-Hosting](Self-Hosting.md).

### 10. Where to go next

- Lexer and parser details → [Lexer and Parser](Frontend-Lexer-and-Parser.md)
- Type rules and monomorphization → [Type Checker](Type-Checker.md)
- LLVM emission and FFI → [Codegen and LLVM](Codegen-and-LLVM-FFI.md)
- The language rules themselves → [Language Reference](Language-Reference.md)
- CLI, optimization levels and the build cache → [CLI and Tooling](CLI-and-Tooling.md)
- Test layout and CI → [Testing and CI](Testing-and-CI.md)

---

## 源文件 / Source files

- [src/lib.rs](../src/lib.rs) — pipeline entry points, `Diag`, import loading, clang linking
- [src/main.rs](../src/main.rs) — CLI commands, option parsing, the shared build cache
- [src/files.rs](../src/files.rs) — the thread-local file-name registry
- [src/lexer.rs](../src/lexer.rs) — the lexer stage and its `Diag` positions
- [src/parser.rs](../src/parser.rs) — the parser stage and its `Diag` positions
- [src/ast.rs](../src/ast.rs) — `Program` / `FnDecl` / `Stmt` / `Expr` / `Pos` shapes
- [src/typecheck.rs](../src/typecheck.rs) — check passes, `FnSig`, monomorphization, `call_map`
- [src/codegen.rs](../src/codegen.rs) — `build_module` phase order, target init, pass pipeline, emit
- [src/llvm.rs](../src/llvm.rs) — the hand-written LLVM-C FFI declarations
- [src/hashing.rs](../src/hashing.rs) — `FastBuild` / `FastHasher`
- [src/platform.rs](../src/platform.rs) — extensions, stack link flag, LLVM lib-name probing
- [tests/pipeline.rs](../tests/pipeline.rs) — end-to-end tests (97 `#[test]`s) incl. the import suite
- [Cargo.toml](../Cargo.toml) — version baseline v0.26.3, no `[dependencies]`
- [Cargo.lock](../Cargo.lock) — the single-package dependency closure
- [CONTRIBUTING.md](../CONTRIBUTING.md) — repository layout, debug switches, design rules
- [docs/spec.md](../docs/spec.md) — the language spec (title and some sections are stale)
- [docs/selfhost.md](../docs/selfhost.md) — the self-hosting plan (older than the current state)
- [README.md](../README.md) — project overview (the Status section is stale)
