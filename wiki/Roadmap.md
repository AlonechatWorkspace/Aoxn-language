# 路线图 · Roadmap

> **中文**：Aoxn 从 v0.7 到 v0.26.3 的实际主线、正在收尾的缺口、语言 / 平台 / Web 三条前进路线，以及仍需拍板的设计问题。
> **English**: The actual main line from v0.7 to v0.26.3, the gaps still being closed, the language / platform / Web routes ahead, and the design questions that still need a decision.

## 中文

**事实基线**：版本历史的权威来源是 [`CHANGELOG.md`](../CHANGELOG.md)；[`README.md`](../README.md) 的 Status 段仍写着
"v0.7 · 70/70 tests"，[`docs/spec.md`](../docs/spec.md) 的首行仍标称 v0.9——**这两处版本信息已落后**。本页所有"已完成"都以
CHANGELOG 的版本条目为据，所有"未实现 / 部分实现"都以 v0.26.3 的源码与 `tests/pipeline.rs`（97 个端到端测试）为据；凡标注
"实测"的结论都用 `target/release/aoxn.exe` 在本仓库 v0.26.3 上跑过探针程序。术语含义见 [术语表](Glossary.md)，
自举细节见 [自举](Self-Hosting.md)，平台细节见 [平台支持](Platform-Support.md)。

### 一、已完成里程碑（0.7 → 0.26.3）

以下每一行都对应 `CHANGELOG.md` 里确实存在的版本条目；0.1–0.6 的序幕单独列出，方便理解后续里程碑的出发点。

| 版本 | 里程碑 | 一句话内容 |
|---|---|---|
| 0.1–0.6 | 序幕（编译器成形） | 初始编译器（lexer → parser → 严格 typecheck → LLVM O3 → 目标文件 → clang 链接）、Python 式语法与布局 token、数组与结构体的值语义、字符串、`for`/`break`/`continue`/f-string/`str()`、`extern def` FFI 与 stdlib 初版。 |
| **0.7.0** | 泛型与单态化 | `def sort[T, N](arr: [T; N])`：类型参数 + 数组长度参数，每个调用点单态化出确定性实例名（如 `sort.i.8`），长度参数在函数体内可当 `int` 常量；stdlib 用泛型重写，取代定长 `_8` 版本。 |
| **0.8.0** | 导入 / 模块系统 | `import "相对路径"`：按导入者所在目录解析、规范化路径 include-once、循环导入报完整链条；诊断开始带文件名（`--json` 同样）。 |
| **0.8.1** | FFI 链接标志与自举可行性 | `-l NAME` / `-L DIR` 透传给 clang；`examples/ffi_llvm.ax` 用 Aoxn 驱动 LLVM-C 并产出真实 IR；`docs/selfhost.md` 给出能力矩阵、差距分析与分阶段自举方案。 |
| **0.9.0** | 原始内存与动态基础 | `load_i64`/`store_i64`、`load_f64`/`store_f64`、`load_u8`/`store_u8`、`as_string`/`as_ptr`；stdlib 增加 `Vec`（8 字节槽、write-back 风格 `v = vec_push(v, x)`）、字节缓冲、字符分类、`read_file`/`write_file`、`system`。 |
| **0.10.0** | 改名 + 自举阶段 1 | 项目由 Axon 改名为 Aoxn；Aoxn 写的词法器（`selfhost/lexer.ax`）以精确 token 流测试验证。 |
| **0.11.0** | 自举阶段 2 / 3 首片 | Aoxn 写的解析器（arena AST、全语法移植）与检查器首片。 |
| **0.12.0** | 自举单态化 | Aoxn 检查器实现同一套单态化与修饰名（`id.i`、`first.i.?.3`），整个 `stdlib.ax` 能被 Aoxn 前端处理。 |
| **0.13.0** | 自举 codegen 首片 | `selfhost/codegen.ax` 驱动 LLVM-C 产出目标文件：int/void 函数、算术与比较、`if`/`while`/`for`、`break`/`continue`、直接调用与单态化实例。 |
| **0.14.0** | 自举 loader | `selfhost/load.ax`：相对导入、include-once、循环检测，多文件解析进同一个 arena。 |
| **0.15.0 / 0.16.0** | 自举 codegen：bool、print、字符串 | `bool`（i1）与 `print`、Windows 二进制 stdout（`_setmode`）、字符串字面量/`len`/`str`/拼接/`strcmp`，f-string 由 parser 脱糖后直接可用。 |
| **0.17.0** | 自举 driver 闭环 | `selfhost/driver.ax`：load → check → codegen → `system("clang ...")`，从真实 `.ax` 产出可执行文件（`selfhost_driver_links_hello`）。 |
| **0.18.0** | 自举 codegen：浮点 | 浮点字面量、局部/参数/返回、`fadd`/`fsub`/`fmul`/`fdiv`、`fneg`、六种有序比较、`%f` 打印与 `str(float)`。 |
| **0.19.0** | 自举 codegen：结构体 | 两阶段命名 LLVM 结构体、字段 GEP 读写、值语义 `memcpy`；结构体参数按指针传递、返回走 sret（这一 ABI 后来在 v0.26.0 反向移植到 Rust 侧）。 |
| **0.20.0** | CPU 目标与 codegen 优化 | `--cpu`/`AOXN_CPU`（当时的说明是 `native` 启用宿主 SIMD、默认 generic 保证可复现；但 v0.26.3 实测 `--cpu native` 会被 LLVM 拒绝——C API 不解析 `native`，要用真实 CPU 名如 `skylake`，见 [命令行与工具链](CLI-and-Tooling.md)）；`int` 算术发 `nsw`、聚合 GEP 发 `inbounds`；字符串长度缓存让累加拼接从 O(n²) 降到 O(总字节)。 |
| **0.21.0** | 自举 codegen：数组与原始内存 | 数组类型/字面量/`[e] * N` 运行时填充/索引读写/`len`/`for x in arr`/数组参数与返回，加上全部原始内存内建；Aoxn driver 能编译导入真 stdlib 的程序，产物输出与退出码与 Rust 编译器一致。 |
| **0.22.0** | 自举固定点（行为级） | `selfhost/driver_self_demo.ax` 编译整个自举编译器（约 7k 行）得到 stage-2；stage-2 编译 stdlib 程序的 stdout 与退出码与 Rust 编译器一致；同时能编译自己的前端与 `examples/` 全部 11 个程序。 |
| **0.23.0** | 编译器性能与代码质量（P2 第一批） | `AOXN_TIME=1` 阶段计时、零依赖 FxHash 风格 hasher（`src/hashing.rs`）、O(1) 查找表、词法器重构为分类扫描器。 |
| **0.24.0** | 固定点升到产物级（IR） | 自举侧新增 `aoxn ir`（`gen_ir_text`/`emit_ir`）；`selfhost_driver_self_compiles` 开始对比两侧 IR 字节，逐字节一致。 |
| **0.25.0** | 固定点升到对象级（COFF） | 两侧 COFF 目标文件逐字节一致；固定点夹具加入嵌套聚合（二维数组、含数组字段的结构体、子数组传参、字面量传数组参数等）。 |
| **0.26.0** | 编译时间塌缩（聚合 ABI） | 聚合改为按指针传参 + sret 返回、聚合在 codegen 中以地址表示、`memcpy` 尺寸变为整型常量：7k 行自举输入的 codegen 32.0s → ~3.3s；新增 `AOXN_PASSES`。 |
| **0.26.1** | 跨平台迁移 | `src/platform.rs` + `build.rs` 平台化（探测 `libLLVM-C`/`libLLVM-XX`/`libLLVM`、发 rpath）、注册 AArch64 后端、非 Windows 用 PIC、`target_os()` 内建、四平台 CI 矩阵（windows-latest / ubuntu-latest / macos-13 / macos-14）。 |
| **0.26.2** | 优化级别与构建缓存（Tier-2 首次全绿） | `--O0`（真 fast-isel）/`--O1`/`--O2`/`--O3`；`aoxn run` 内容哈希缓存；`platform::llvm_link_name()` 探测链接名；POSIX 为每个 `-L` 加 rpath；`system_exit_code` 归一化；Linux/macOS CI 首次全绿；测试 93 → 97。 |
| **0.26.3** | 当前版本（编译速度收尾） | `aoxn build` 接入与 `run` 同一份缓存（重复构建 ~0.8–1.6s → ~0.2s）；前端微优化（`VecDeque` 实例队列、`StructInfo { fields, index }`、`type_size` 记忆化、`AOXN_TC_TRACE`/`AOXN_CG_TRACE` 每编译只读一次）；补齐 CONTRIBUTING / CODE_OF_CONDUCT / SECURITY 与 issue 模板。 |

Web 基准套件不在 `CHANGELOG.md` 的版本条目里，它的进度记在 [docs/web-platform-plan.md](../docs/web-platform-plan.md)
与 [`web/README.md`](../web/README.md)，见本文第五节。

### 二、进行中 / 待完成

| 事项 | 现状 | 缺口（**计划中，尚未实现**） | 依据 |
|---|---|---|---|
| **自举侧的 CLI 语义** | `selfhost/driver.ax` 已能 load → check → codegen → `system("clang ...")` 链接并产出可执行文件，行为与 Rust 编译器一致。 | 它还**不是**命令行前端：语言没有 argv（全仓 `*.ax` 里 `argv` 零命中），所以各 `driver_*_demo.ax` 把输入/输出路径写死在源码里，无法接受用户给定的文件名、`-o`、子命令或 `--json`。 | [docs/platform-support.md](../docs/platform-support.md) §7 遗留、[selfhost/driver.ax](../selfhost/driver.ax) |
| **非 Windows 的逐字节固定点** | 固定点算法与平台无关（IR 文本 + 对象字节），v0.26.1 起对象可以是 ELF/Mach-O；Tier 2 的 CI 会跑 selfhost 系列测试。 | `selfhost_driver_self_compiles` 在 `C:/Program Files/LLVM/lib/LLVM-C.lib` 不存在时**自行跳过**（[tests/pipeline.rs](../tests/pipeline.rs) 里的显式 return），且 `selfhost/driver_self_demo.ax` 仍硬编码链接名 `LLVM-C`；因此"逐字节一致"目前只在 Windows 实际验证。解除它需要给自举 demo 传入库名（依赖 argv）或给 `extern def` 增加链接名别名机制。 | [docs/platform-support.md](../docs/platform-support.md) §7、[tests/pipeline.rs](../tests/pipeline.rs)、[selfhost/driver_self_demo.ax](../selfhost/driver_self_demo.ax) |
| **自举路径的 f-string 与浮点格式化覆盖** | 自举 codegen 已实现 f-string（parser 脱糖）与 `str(float)`（`snprintf("%f")`）；`selfhost_codegen_int_slice` 的固定用例里含 `print(f"n squared = ...")` 与 `print("f=" + str(f))`，并与 Rust 编译器做 stdout 逐字节对比。 | 承载**逐字节固定点**的夹具 `STDLIB_USE_PROG` 只用 `print(hypot(3.0, 4.0))` 打印浮点，不含 f-string、也不含 `str(float)`；即"编译器编译自己"的链条尚未覆盖这两条格式化路径。 | [selfhost/codegen.ax](../selfhost/codegen.ax)、[tests/pipeline.rs](../tests/pipeline.rs)、[selfhost/driver_self_demo.ax](../selfhost/driver_self_demo.ax) |
| **文档同步** | 平台声明与工具链契约已进入 `docs/spec.md`（v0.26.1 / v0.26.2），`CONTRIBUTING.md` 记录了聚合 ABI 不变量与"固定点仅 Windows"的限制。 | `README.md` 的 Status 段、`docs/spec.md` 的版本号与 Statements 段（"`while` 是唯一的循环"）、Program structure 段的 extern `string` 限制、`docs/selfhost.md` 的进度与"剩余工作"段仍是旧稿，需要一次性校对。 | [README.md](../README.md)、[docs/spec.md](../docs/spec.md)、[docs/selfhost.md](../docs/selfhost.md) |

### 三、语言路线图（`docs/spec.md` 的 Roadmap 逐条核对）

先说明**已过时之处**：`docs/spec.md` 标称 v0.9，其 Statements 段仍写 "`while` 是唯一的循环（还没有 `for`）"，而 `for`
早在 v0.5 就已实现（[src/lexer.rs](../src/lexer.rs) 的 `Tok::For`、[src/parser.rs](../src/parser.rs) 的 `Stmt::For`，
测试 `for_range_forms` / `for_array_iteration` / `for_nested_and_reuse`）；同一份文档的 Program structure 段还写 extern
"不能用 `string` 参数/返回值"，而 stdlib 的 `fopen`/`system` 正是 `string` 参数（实测
`extern def strlen(s: string) -> int` 可编译可运行）——**该文档这两处已过时**，应以源码与测试为准。以下七条是 spec.md
Roadmap 的原文条目，逐条给出 v0.26.3 的现状核对。

| spec.md 的 Roadmap 条目 | 现状 | 核对依据（v0.26.3 源码 / 测试 / 实测） |
|---|---|---|
| 1. 模块限定（`lib.sort(...)`）、选择性导入、包布局 | **未实现** | `import` 只接受字符串路径（实测 `import stdlib` 报 `[parse] expected a string path after 'import'`），导入把文件并入**单一命名空间**，没有模块对象、`::` 或选择性导入语法，也没有包布局概念。计划中，尚未实现。 |
| 2. 字符串索引 / 迭代（需要 `char` 或子串切片） | **未实现** | 实测 `s[0]` 报 `[type] cannot index a value of type string (only [T; N] arrays are indexable)`；关键字表里没有 `char`（实测 `c: char = 65` 报 "cannot initialize 'c: char' with an expression of type int"，即 `char` 被当作未知结构体名）。计划中，尚未实现。 |
| 3. f-string 格式说明符（`{x:.2f}`）与 f-string 多行 | **未实现；格式说明符被"静默忽略"** | 脱糖时每个插值新建子解析器且只取**第一个完整表达式**，不检查剩余 token（[src/parser.rs](../src/parser.rs) 的 `desugar_fstring` → `sub.expr()`）：实测 `f"{x:.2f}"`、`f"{x!r}"` 都能编译并只输出 `str(x)` 的默认格式（`1.500000`），而 `f"{x:@@@}"` 在词法阶段报 `unexpected character '@'`——说明格式说明符既未实现、也没有诊断。跨行 f-string 实测可编译（字面量按字符扫描，换行被保留），但规范未定义、测试未覆盖，不能算"已支持"。计划中，尚未实现。 |
| 4. 内存：字符串驻留或 arena 释放（现在拼接会泄漏） | **未实现（按设计泄漏）** | 规范明说拼接结果堆分配且故意不释放（"no GC yet"，安全的前提是字符串不可变）；`web/http_buf.ax` 的字节缓冲正是为规避逐请求拼接而采用的惯用法。[docs/spec.md](../docs/spec.md)、[docs/web-benchmark.md](../docs/web-benchmark.md) |
| 5. 自举：用 Aoxn 重写编译器 | **部分完成**（固定点已达成；CLI 语义未移植） | v0.22 行为级、v0.24 IR 逐字节、v0.25 对象逐字节；剩余缺口见第二节。 |
| 6. 标准库扩展：容器、IO、加密 | **部分完成** | 已有：`Vec`（8 字节槽、write-back）、字节缓冲（`buf_new`/`fill_zero`/`str_get`）、字符分类、`read_file`/`write_file`、`system`/`system_exit_code`、泛型 `sort`/`linear_search`/`binary_search`/`max_of`/`min_of`/`reverse`/`sum_int`/`sum_float`、数学函数。缺：映射/集合等通用容器、加密、迭代器式 API。计划中，尚未实现。 |
| 7. 顶层语句作为隐式 `main`（脚本模式） | **未实现** | 顶层只允许 `import` / `struct` / `def` / `extern def`：实测顶层 `print("hi")` 报 `[parse] expected 'import', 'def' or 'struct' at top level`，`tests/pipeline.rs` 的 `rejects_top_level_statement` 把这一行为固定为规范。计划中，尚未实现。 |

### 四、平台路线（M0–M5 与遗留项）

[docs/platform-migration-plan.md](../docs/platform-migration-plan.md) 把跨平台迁移拆成 M0–M5；下表是各里程碑在 v0.26.3 的落地情况。

| 里程碑 | 内容 | 现状 |
|---|---|---|
| **M0** | Rust 侧平台抽象 + `build.rs` 平台化 | **已完成**：`src/platform.rs` 集中扩展名、栈标志、平台判定与 LLVM 候选目录；`build.rs` 按 OS 探测 `libLLVM-C`/`libLLVM-XX`/`libLLVM` 并发 rpath；`AXON_LLVM_DIR` 作为 legacy alias 保留。 |
| **M1** | Linux x86_64 打通 + CI | **已完成**：v0.26.1 加入矩阵，v0.26.2 首次全绿（`llvm_link_name()`、`join_paths`、rpath、`system_exit_code` 等修补见 [docs/platform-support.md](../docs/platform-support.md) §7）。 |
| **M2** | macOS Intel 打通 + CI | **已完成**：`build.rs` 覆盖 brew 布局，`macos-13` job 在矩阵内。 |
| **M3** | Apple Silicon（AArch64 后端）+ CI | **已完成**：Rust 侧与自举侧都注册 `LLVMInitializeAArch64{TargetInfo,Target,TargetMC,AsmPrinter}`（自举侧是 v0.26.2 补的），`macos-14` job 在矩阵内。 |
| **M4** | 自举侧对齐（含 `target_os()` 语言设施） | **部分完成**：`target_os()` 已落地且两侧对称折叠；`_setmode`（自举 codegen）与 `/STACK`（自举 driver）已按平台分支；Tier-2 CI 能跑 selfhost 测试。**未完成**：非 Windows 的逐字节固定点（见第二节）——**计划中，尚未实现**。 |
| **M5** | 文档、spec 声明、发布 | **已完成**：`docs/spec.md` 写入 Tier 1 / Tier 2 平台表与工具链；CHANGELOG v0.26.1 记录迁移；CI 矩阵固化为 windows-latest + ubuntu-latest + macos-13 + macos-14。 |

遗留项与明确非目标：

- `selfhost/driver_self_demo.ax` 仍硬编码链接名 `LLVM-C`，固定点测试仍以 `C:/Program Files/LLVM/lib/LLVM-C.lib`
  的存在为运行前提，所以**逐字节固定点目前只在 Windows 验证**。解除需要 argv 或 `extern def` 链接名别名——
  **计划中，尚未实现**。
- 非目标（[docs/platform-migration-plan.md](../docs/platform-migration-plan.md) §0）：交叉编译（`--target` 指定非宿主
  平台）、MinGW 工具链、32 位平台、官方安装包分发——这些都留到平台声明稳定后另行立项，**计划中，尚未实现**。

### 五、Web 平台路线

设计讨论与已定决策见 [docs/web-platform-plan.md](../docs/web-platform-plan.md)。**已决策**：TypeScript 语法完整兼容走
方案 A（新增 TS 前端 → 复用现有编译管线，语义按 TS 对齐分期）；包管理走 A2（**独立**包管理：自有 manifest、自有
lockfile、自建 registry，配 npm 桥接）；样式走 B1（完整兼容 CSS 打底 + Tailwind 工具链 + CSS Modules）。现有
`import "相对路径"` 架构将被 TS 式 `import`/`export` **替换**（不是双轨共存），stdlib 与 selfhost 同批迁移，切换
节奏是待决项 C。

| 阶段 | 内容 | 现状 |
|---|---|---|
| **W0** | 可观测性 `/metrics`（RED 指标）+ 功能对等测试 `parity.mjs`；三平台 CI（`web-bench.yml`）功能验证 + 参考基准；设计文档 | **已完成**（记录在 `docs/web-platform-plan.md` §8 与 [`web/README.md`](../web/README.md)，不在 CHANGELOG 的版本条目里）。 |
| **W1** | TS-M1 前端（语法子集 → 现有管线）+ 新模块系统（`import`/`export`）+ **移除旧 `import`**；stdlib / selfhost 同批迁移 | **计划中，尚未实现**；这是当前下一步（"W1 TS-M1 前端"）。 |
| **W2** | 独立包管理（manifest / lockfile / registry 客户端 + npm 桥接）；CSS 资源管线 + CSS Modules + Tailwind 接入；静态文件服务 + Range + 缓存头 | **计划中，尚未实现**。 |
| **W3** | 基准 v2 全量执行（复杂场景 S0–S4 + 浏览器渲染指标 + 编译时长），Linux/macOS 专用机数据 | **计划中，尚未实现**（依赖 W2）。 |
| **W4** | TS-M2 运行时语义（对象引用语义、闭包、GC 或区域分配起步）→ 团队真实项目迁移验收 | **计划中，尚未实现**（依赖样本项目）。 |
| **TS-M3** | 全量：async/Promise、装饰器、namespace、`lib.dom`/`node` 常用面 | **计划中，尚未实现**（"完整兼容 TypeScript 语法"的达标线）。 |

剩余待决（都需要拍板，**计划中，尚未实现**）：旧 `import` 的移除节奏（随 W1 一次切换还是先双轨）；npm 桥接方案
（一次性导入工具还是注册表代理层）；TS-M1 的 2–3 个团队真实验收样本；页面渲染指标是否用 Playwright + Chrome
（FCP/LCP/TTI）；正式基准数据用 CI 参考值还是专用机。

### 六、开放问题与需要设计决策的点

1. **条件编译设施**：今天语言里唯一的平台感知手段是 `target_os()`（编译期折叠成模块内私有字符串常量，v0.26.1），
   没有 `cfg` / `#if` / 属性机制，也没有 `getenv`（[docs/platform-migration-plan.md](../docs/platform-migration-plan.md)
   §6 明确记录了"无 cfg、无 getenv"的核实结论与"不引入 getenv"的理由；当前源码中确实没有 `getenv`）。是否需要
   "按平台排除整段代码"这类更强设施，还是继续用 `if target_os() == "windows":` 分支，需要拍板——**计划中，尚未实现**。
2. **`char` 类型**：它是字符串索引 / 迭代（语言路线第 2 条）的前置。要决定是引入一等 `char`（新类型 + 索引语义 +
   codegen 表示 + 自举移植 + 固定点同步），还是先只提供 stdlib 缓冲函数——[docs/selfhost.md](../docs/selfhost.md)
   的 B3 建议是"先 buffer 函数，`char` 之后"——**计划中，尚未实现**。
3. **`enum` / `match`（sum types）**：AST 与错误处理的表达力瓶颈。实测 `enum Color:` 报
   `[parse] expected 'import', 'def' or 'struct' at top level`，`match x:` 不是关键字（被当成普通语句解析并报
   `expected a type: int | float | bool | string | void | name | [T; N]`）。`docs/selfhost.md` 的 B2 结论是"先用
   arena + tagged structs，`match` 语法以后再说"，自举编译器目前正是这么写的；要引入它意味着新语法 + 类型检查 +
   codegen + 自举移植 + 固定点同步——**计划中，尚未实现**。
4. **内存回收策略**：字符串拼接结果现在按设计不释放，`Vec` 的释放责任在调用者。是否引入 arena、区域分配或引用
   计数，决定了语言路线第 4 条与标准库容器扩展的最终形态——**计划中，尚未实现**。
5. **自举完成后的分工**：[docs/selfhost.md](../docs/selfhost.md) §4 的定位是 Rust 编译器在 stage 2 稳定后"退化为
   测试预言机"。是否把自举编译器升级为默认实现、Rust 编译器是否长期保留为 CI oracle，需要拍板——**计划中，尚未实现**。

## English

**Basis of fact**: the authoritative version history is [`CHANGELOG.md`](../CHANGELOG.md); the Status section of
[`README.md`](../README.md) still says "v0.7 · 70/70 tests" and the first line of [`docs/spec.md`](../docs/spec.md) is still
labelled v0.9 — **both version statements are out of date**. Everything marked "done" below is backed by a CHANGELOG
version entry; everything marked "not implemented" or "partial" is backed by the v0.26.3 sources and
`tests/pipeline.rs` (97 end-to-end tests). Claims marked "measured" were probed with `target/release/aoxn.exe` on this
v0.26.3 working tree. Term definitions live in the [Glossary](Glossary.md), self-hosting details in
[Self-Hosting](Self-Hosting.md), platform details in [Platform Support](Platform-Support.md).

### 1. Completed milestones (0.7 → 0.26.3)

Every row corresponds to a version entry that really exists in `CHANGELOG.md`; the 0.1–0.6 prologue is listed separately
because later milestones build on it.

| Version | Milestone | What it delivered |
|---|---|---|
| 0.1–0.6 | Prologue (the compiler takes shape) | The initial compiler (lexer → parser → strict typecheck → LLVM O3 → object file → clang link), Python-style syntax with layout tokens, value semantics for arrays and structs, strings, `for`/`break`/`continue`/f-strings/`str()`, `extern def` FFI and the first stdlib. |
| **0.7.0** | Generics and monomorphization | `def sort[T, N](arr: [T; N])`: type parameters plus an array-length parameter, one deterministic monomorphized instance per call site (`sort.i.8`), the length parameter usable as an `int` constant in the body; the stdlib was rewritten with generics, replacing the fixed-size `_8` functions. |
| **0.8.0** | Import / module system | `import "relative/path"`: paths resolved against the importing file, include-once per canonical path, circular imports reported with the full chain; diagnostics started naming their source file (also under `--json`). |
| **0.8.1** | FFI link flags and the self-hosting assessment | `-l NAME` / `-L DIR` forwarded to clang; `examples/ffi_llvm.ax` drives LLVM-C from Aoxn and emits real IR; `docs/selfhost.md` records the capability matrix, gap analysis and staged bootstrap plan. |
| **0.9.0** | Raw memory and the dynamic foundation | `load_i64`/`store_i64`, `load_f64`/`store_f64`, `load_u8`/`store_u8`, `as_string`/`as_ptr`; stdlib gains `Vec` (8-byte slots, write-back style `v = vec_push(v, x)`), byte buffers, char classification, `read_file`/`write_file` and `system`. |
| **0.10.0** | Rename + bootstrap stage 1 | Axon becomes Aoxn; the Aoxn-written lexer (`selfhost/lexer.ax`) is verified by an exact token-stream test. |
| **0.11.0** | Bootstrap stages 2 / first slice of 3 | The Aoxn-written parser (arena AST, whole grammar ported) and the first slice of the checker. |
| **0.12.0** | Self-hosted monomorphization | The Aoxn checker implements the same monomorphization and mangled names (`id.i`, `first.i.?.3`), and the whole `stdlib.ax` typechecks in the Aoxn front end. |
| **0.13.0** | First self-hosted codegen slice | `selfhost/codegen.ax` drives LLVM-C to emit an object file: int/void functions, arithmetic and comparisons, `if`/`while`/`for`, `break`/`continue`, direct calls and monomorphized instances. |
| **0.14.0** | Self-hosted loader | `selfhost/load.ax`: relative imports, include-once, cycle detection, several files parsed into one shared arena. |
| **0.15.0 / 0.16.0** | Self-hosted codegen: bool, print, strings | `bool` (i1) and `print`, binary stdout on Windows (`_setmode`), string literals / `len` / `str` / concatenation / `strcmp`; f-strings work through parser desugaring. |
| **0.17.0** | Self-hosted driver closes the loop | `selfhost/driver.ax`: load → check → codegen → `system("clang ...")`, producing an executable from a real `.ax` file (`selfhost_driver_links_hello`). |
| **0.18.0** | Self-hosted codegen: floats | Float literals, locals/params/returns, `fadd`/`fsub`/`fmul`/`fdiv`, `fneg`, all six ordered comparisons, `%f` printing and `str(float)`. |
| **0.19.0** | Self-hosted codegen: structs | Two-phase named LLVM structs, field GEP read/write, value-semantics `memcpy`; struct parameters passed as pointers and struct returns through sret (an ABI later back-ported to the Rust side in v0.26.0). |
| **0.20.0** | Target CPU and codegen optimization | `--cpu`/`AOXN_CPU` (documented at the time as "`native` enables host SIMD, the generic default keeps output reproducible"; measured at v0.26.3, however, LLVM rejects `--cpu native` — the C API does not resolve `native`, so pass a real CPU name such as `skylake`, see [CLI and Tooling](CLI-and-Tooling.md)); `int` arithmetic emits `nsw` and aggregate GEPs emit `inbounds`; string length caching takes accumulator loops from O(n²) to O(total bytes). |
| **0.21.0** | Self-hosted codegen: arrays and raw memory | Array types/literals, `[e] * N` runtime fill, indexing read/write, `len`, `for x in arr`, array params and returns, plus every raw-memory builtin; the Aoxn driver compiles programs importing the real stdlib with output and exit code matching the Rust compiler. |
| **0.22.0** | Self-hosting fixed point (behavioral) | `selfhost/driver_self_demo.ax` compiles the entire self-hosting compiler (~7k lines) into stage 2; stage 2 compiles a stdlib program whose stdout and exit code match the Rust compiler's, and can also compile its own front end plus all 11 programs in `examples/`. |
| **0.23.0** | Compiler performance and code quality (P2, batch 1) | `AOXN_TIME=1` stage timing, a zero-dependency FxHash-style hasher (`src/hashing.rs`), O(1) lookup tables, and a lexer restructured into per-category scanners. |
| **0.24.0** | Fixed point raised to the artifact level (IR) | The self-hosted side gains `aoxn ir` (`gen_ir_text`/`emit_ir`); `selfhost_driver_self_compiles` starts comparing the two sides' IR bytes, which are byte-identical. |
| **0.25.0** | Fixed point raised to the object level (COFF) | The two sides' COFF object files are byte-identical; the fixed-point fixture gains nested aggregates (2D arrays, structs with array fields, sub-array arguments, literals passed to array parameters, …). |
| **0.26.0** | Compile times collapse (aggregate ABI) | Aggregates cross function boundaries by pointer, aggregate returns use sret, aggregates are represented by address in codegen, and `memcpy` sizes became plain integer constants: codegen for the 7k-line self-hosting input went 32.0s → ~3.3s; `AOXN_PASSES` was added. |
| **0.26.1** | Cross-platform migration | `src/platform.rs` plus a platformized `build.rs` (probing `libLLVM-C`/`libLLVM-XX`/`libLLVM`, emitting an rpath), the AArch64 backend registered, PIC off Windows, the `target_os()` builtin, and a four-platform CI matrix (windows-latest / ubuntu-latest / macos-13 / macos-14). |
| **0.26.2** | Optimization levels and the build cache (Tier 2 green at last) | `--O0` (real fast-isel) / `--O1` / `--O2` / `--O3`; the `aoxn run` content-hash cache; `platform::llvm_link_name()` probing the link name; an rpath per `-L` on POSIX; `system_exit_code` normalization; Linux/macOS CI green for the first time; the suite grew from 93 to 97 tests. |
| **0.26.3** | Current version (compile-speed follow-through) | `aoxn build` joins the same cache as `run` (a repeated build went ~0.8–1.6s → ~0.2s); front-end micro-optimizations (`VecDeque` instance queue, `StructInfo { fields, index }`, memoized `type_size`, `AOXN_TC_TRACE`/`AOXN_CG_TRACE` read once per compile); CONTRIBUTING / CODE_OF_CONDUCT / SECURITY and the issue templates added. |

The web benchmark suite has no CHANGELOG version entry of its own: its progress is recorded in
[docs/web-platform-plan.md](../docs/web-platform-plan.md) and [`web/README.md`](../web/README.md), and is summarized in
section 5 below.

### 2. In progress / outstanding

| Item | Where it stands | The gap (**planned, not implemented**) | Source |
|---|---|---|---|
| **CLI semantics on the self-hosted side** | `selfhost/driver.ax` already orchestrates load → check → codegen → `system("clang ...")` and produces an executable whose behavior matches the Rust compiler's. | It is **not** a command-line front end yet: the language has no argv (zero hits for `argv` across the repo's `*.ax` files), so every `driver_*_demo.ax` hardcodes its input/output paths and cannot accept a user-supplied file name, `-o`, subcommand or `--json`. | [docs/platform-support.md](../docs/platform-support.md) §7, [selfhost/driver.ax](../selfhost/driver.ax) |
| **Byte-exact fixed point off Windows** | The fixed-point algorithm is platform-independent (IR text plus object bytes), and since v0.26.1 the object can be ELF/Mach-O; Tier-2 CI does run the selfhost test family. | `selfhost_driver_self_compiles` **skips itself** unless `C:/Program Files/LLVM/lib/LLVM-C.lib` exists (an explicit early return in [tests/pipeline.rs](../tests/pipeline.rs)), and `selfhost/driver_self_demo.ax` still hardcodes the link name `LLVM-C`; the byte-for-byte comparison therefore only runs on Windows today. Lifting it needs a library name passed into the demo (blocked on argv) or a link-name alias for `extern def`. | [docs/platform-support.md](../docs/platform-support.md) §7, [tests/pipeline.rs](../tests/pipeline.rs), [selfhost/driver_self_demo.ax](../selfhost/driver_self_demo.ax) |
| **f-string and float-formatting coverage on the self-hosted path** | The self-hosted codegen implements f-strings (parser desugaring) and `str(float)` (`snprintf("%f")`); the fixed case in `selfhost_codegen_int_slice` contains `print(f"n squared = ...")` and `print("f=" + str(f))` and compares stdout byte-for-byte against the Rust compiler. | The fixture that carries the **byte-exact fixed point**, `STDLIB_USE_PROG`, only prints a float through `print(hypot(3.0, 4.0))` — it contains neither an f-string nor `str(float)`, so the "compiler compiles itself" chain does not yet cover those two formatting paths. | [selfhost/codegen.ax](../selfhost/codegen.ax), [tests/pipeline.rs](../tests/pipeline.rs), [selfhost/driver_self_demo.ax](../selfhost/driver_self_demo.ax) |
| **Documentation sync** | The platform declaration and the tooling contract made it into `docs/spec.md` (v0.26.1 / v0.26.2), and `CONTRIBUTING.md` records the aggregate-ABI invariants and the Windows-only fixed point. | The Status section of `README.md`, the version number and Statements section of `docs/spec.md` ("`while` is the only loop"), its Program structure section's extern `string` restriction, and the progress/remaining-work sections of `docs/selfhost.md` are still the old drafts and need one coordinated pass. | [README.md](../README.md), [docs/spec.md](../docs/spec.md), [docs/selfhost.md](../docs/selfhost.md) |

### 3. Language roadmap (every `docs/spec.md` Roadmap item checked against reality)

**Already stale**: `docs/spec.md` is labelled v0.9 and its Statements section still says "`while` is the only loop (no
`for` yet)", while `for` landed back in v0.5 ([`Tok::For`](../src/lexer.rs) in the lexer, `Stmt::For` in
[src/parser.rs](../src/parser.rs), and the `for_range_forms` / `for_array_iteration` / `for_nested_and_reuse` tests); the
same document's Program structure section also says externs "cannot use `string` params/returns yet", although the stdlib's
`fopen`/`system` take `string` parameters (measured: `extern def strlen(s: string) -> int` compiles and runs) — **the
document is out of date in both places**, and the source and tests are authoritative. The seven rows below are spec.md's
own Roadmap entries, each checked against v0.26.3.

| spec.md Roadmap item | Status | Evidence (v0.26.3 source / tests / measurement) |
|---|---|---|
| 1. Module qualification (`lib.sort(...)`), selective imports, package layout | **Not implemented** | `import` accepts a string path only (measured: `import stdlib` reports `[parse] expected a string path after 'import'`) and merges a file into **one namespace**; there is no module object, no `::` and no selective-import syntax, and no notion of a package layout. Planned, not implemented. |
| 2. String indexing / iteration (needs a `char` type or substring slices) | **Not implemented** | Measured: `s[0]` reports `[type] cannot index a value of type string (only [T; N] arrays are indexable)`; `char` is not in the keyword table either (measured: `c: char = 65` reports "cannot initialize 'c: char' with an expression of type int", i.e. `char` is treated as an unknown struct name). Planned, not implemented. |
| 3. Format specifiers in f-strings (`{x:.2f}`) and multi-line f-strings | **Not implemented; format specifiers are silently ignored** | Desugaring builds a sub-parser per interpolation that takes only the **first complete expression** and never checks for leftover tokens ([src/parser.rs](../src/parser.rs), `desugar_fstring` → `sub.expr()`): measured, `f"{x:.2f}"` and `f"{x!r}"` both compile and print `str(x)`'s default formatting (`1.500000`), while `f"{x:@@@}"` fails at the lexer with `unexpected character '@'` — so format specifiers are neither implemented nor diagnosed. A multi-line f-string compiles in practice (literals are scanned character by character and the newline is preserved), but it is unspecified and untested, so it does not count as supported. Planned, not implemented. |
| 4. Memory: string interning or arena freeing (concatenation currently leaks) | **Not implemented (leaks by design)** | The spec states that concat results are heap-allocated and intentionally never freed ("no GC yet", sound because strings are immutable); the byte buffers in `web/http_buf.ax` exist precisely to avoid per-request concatenation. See [docs/spec.md](../docs/spec.md) and [docs/web-benchmark.md](../docs/web-benchmark.md). |
| 5. Self-hosting: rewrite the compiler in Aoxn | **Partial** (fixed point reached; CLI semantics not ported) | v0.22 behavioral, v0.24 byte-exact IR, v0.25 byte-exact objects; the remaining gaps are in section 2. |
| 6. Standard library expansion: containers, IO, crypto | **Partial** | Shipped: `Vec` (8-byte slots, write-back), byte buffers (`buf_new`/`fill_zero`/`str_get`), char classification, `read_file`/`write_file`, `system`/`system_exit_code`, the generic `sort`/`linear_search`/`binary_search`/`max_of`/`min_of`/`reverse`/`sum_int`/`sum_float` and the math helpers. Missing: general containers such as maps and sets, crypto, and iterator-style APIs. Planned, not implemented. |
| 7. Top-level statements as an implicit `main` (script mode) | **Not implemented** | Only `import` / `struct` / `def` / `extern def` are allowed at the top level: measured, a top-level `print("hi")` reports `[parse] expected 'import', 'def' or 'struct' at top level`, and the `rejects_top_level_statement` test pins that behavior as the specification. Planned, not implemented. |

### 4. Platform roadmap (M0–M5 and what is left)

[docs/platform-migration-plan.md](../docs/platform-migration-plan.md) splits the cross-platform migration into M0–M5; the
table shows where each milestone stands in v0.26.3.

| Milestone | Content | Status |
|---|---|---|
| **M0** | Platform abstraction on the Rust side + platformized `build.rs` | **Done**: `src/platform.rs` centralizes extensions, the stack flag, platform predicates and the LLVM candidate directories; `build.rs` probes `libLLVM-C`/`libLLVM-XX`/`libLLVM` per OS and emits an rpath; `AXON_LLVM_DIR` is kept as a legacy alias. |
| **M1** | Linux x86_64 plus CI | **Done**: added to the matrix in v0.26.1 and green for the first time in v0.26.2 (`llvm_link_name()`, `join_paths`, rpath, `system_exit_code` — see [docs/platform-support.md](../docs/platform-support.md) §7). |
| **M2** | macOS Intel plus CI | **Done**: `build.rs` covers the brew layout and the `macos-13` job is in the matrix. |
| **M3** | Apple Silicon (AArch64 backend) plus CI | **Done**: both the Rust side and the self-hosted side register `LLVMInitializeAArch64{TargetInfo,Target,TargetMC,AsmPrinter}` (the self-hosted half arrived in v0.26.2), and the `macos-14` job is in the matrix. |
| **M4** | Aligning the self-hosted side (including the `target_os()` language facility) | **Partial**: `target_os()` landed and folds symmetrically on both sides; `_setmode` (self-hosted codegen) and `/STACK` (self-hosted driver) are platform-gated; Tier-2 CI does run the selfhost tests. **Not done**: the byte-exact fixed point off Windows (see section 2) — planned, not implemented. |
| **M5** | Docs, spec declaration, release | **Done**: `docs/spec.md` carries the Tier 1 / Tier 2 platform table and toolchain notes; CHANGELOG v0.26.1 records the migration; the CI matrix is fixed at windows-latest + ubuntu-latest + macos-13 + macos-14. |

What is left, plus the explicit non-goals:

- `selfhost/driver_self_demo.ax` still hardcodes the link name `LLVM-C`, and the fixed-point test still requires
  `C:/Program Files/LLVM/lib/LLVM-C.lib` to run, so **the byte-exact fixed point is verified on Windows only**. Lifting it
  needs argv or a link-name alias for `extern def` — **planned, not implemented**.
- Non-goals ([docs/platform-migration-plan.md](../docs/platform-migration-plan.md) §0): cross-compilation (`--target` for a
  non-host platform), the MinGW toolchain, 32-bit targets, and official installer distribution — all deferred until the
  platform declaration is stable — **planned, not implemented**.

### 5. Web platform roadmap

The design discussion and the settled decisions are in [docs/web-platform-plan.md](../docs/web-platform-plan.md).
**Decided**: full TypeScript syntax compatibility takes option A (a new TS front end feeding the existing pipeline, with TS
semantics phased in); package management takes A2 (**independent** package management: own manifest, own lockfile, own
registry, plus an npm bridge); styling takes B1 (full CSS compatibility as the base plus the Tailwind toolchain and CSS
Modules). The current `import "relative/path"` architecture is to be **replaced** by TS-style `import`/`export` (not kept
as a parallel track), with the stdlib and selfhost migrated in the same batch; the switchover timing is open decision C.

| Phase | Content | Status |
|---|---|---|
| **W0** | Observability `/metrics` (RED counters) + the functional parity test `parity.mjs`; three-platform CI (`web-bench.yml`) with functional verification and a reference benchmark; the design document | **Done** (recorded in `docs/web-platform-plan.md` §8 and [`web/README.md`](../web/README.md), not as a CHANGELOG version entry). |
| **W1** | The TS-M1 front end (syntax subset → existing pipeline) + the new module system (`import`/`export`) + **removing the old `import`**; stdlib / selfhost migrated in the same batch | **Planned, not implemented**; this is the immediate next step ("W1 TS-M1 front end"). |
| **W2** | Independent package management (manifest / lockfile / registry client + npm bridge); CSS asset pipeline + CSS Modules + Tailwind integration; static file serving + Range + cache headers | **Planned, not implemented.** |
| **W3** | Benchmark v2 in full (complex scenarios S0–S4 + browser rendering metrics + compile times), with dedicated Linux/macOS machines | **Planned, not implemented** (depends on W2). |
| **W4** | TS-M2 runtime semantics (object reference semantics, closures, GC or region allocation to start) → acceptance on a real team project | **Planned, not implemented** (depends on sample projects). |
| **TS-M3** | Everything: async/Promise, decorators, namespaces, the common `lib.dom`/`node` surface | **Planned, not implemented** (the bar for "full TypeScript syntax compatibility"). |

Open decisions (all needing a call, all **planned, not implemented**): the timing of the old `import` removal (one switch
with W1 or a dual track first); the npm bridge approach (a one-shot import tool or a registry proxy); the two or three real
team projects to accept TS-M1 against; whether page rendering metrics use Playwright + Chrome (FCP/LCP/TTI); and whether
the official benchmark numbers come from CI reference runs or dedicated machines.

### 6. Open questions and design decisions

1. **Conditional-compilation facilities**: the only platform-awareness facility in the language today is `target_os()` (a
   compile-time fold to a module-private string constant, v0.26.1). There is no `cfg` / `#if` / attribute mechanism and no
   `getenv` ([docs/platform-migration-plan.md](../docs/platform-migration-plan.md) §6 records the "no cfg, no getenv"
   finding and why a `getenv` heuristic was rejected; the current sources really do contain no `getenv`). Whether to add a
   stronger facility for excluding whole blocks per platform, or to keep writing
   `if target_os() == "windows":` branches, is undecided — **planned, not implemented**.
2. **The `char` type**: it is the prerequisite for string indexing and iteration (language roadmap item 2). The decision is
   between a first-class `char` (a new type plus indexing semantics, codegen representation, a self-hosted port and a fixed
   point to re-verify) and leaving it to stdlib buffer functions for now — the B3 recommendation in
   [docs/selfhost.md](../docs/selfhost.md) is "buffer functions first, `char` later" — **planned, not implemented**.
3. **`enum` / `match` (sum types)**: the expressiveness bottleneck for ASTs and error handling. Measured, `enum Color:`
   reports `[parse] expected 'import', 'def' or 'struct' at top level` and `match x:` is not a keyword (it parses as an
   ordinary statement and reports `expected a type: int | float | bool | string | void | name | [T; N]`). The B2
   conclusion in `docs/selfhost.md` is "arena + tagged structs first, `match` syntax later", which is exactly how the
   self-hosted compiler is written today; introducing it means new
   syntax plus typechecking, codegen, a self-hosted port and a fixed-point sync — **planned, not implemented**.
4. **Memory reclamation strategy**: concat results are currently never freed by design and `Vec` frees are the caller's
   job. Whether to introduce an arena, region allocation or reference counting shapes both language roadmap item 4 and the
   final form of the stdlib's containers — **planned, not implemented**.
5. **Division of labor after self-hosting**: [docs/selfhost.md](../docs/selfhost.md) §4 positions the Rust compiler as
   becoming "a test oracle only" once stage 2 is stable. Whether the self-hosted compiler becomes the default
   implementation, and whether the Rust compiler stays as the CI oracle long term, is undecided — **planned, not
   implemented**.

---

## 源文件 / Source files

- [CHANGELOG.md](../CHANGELOG.md) — the authority for section 1 (every version entry from 0.7.0 to 0.26.3)
- [README.md](../README.md) — project framing (its Status section is stale: v0.7 / 70 tests)
- [docs/spec.md](../docs/spec.md) — the seven language-roadmap items in section 3 (labelled v0.9; its Statements section is stale)
- [docs/selfhost.md](../docs/selfhost.md) — the bootstrap ladder, the stage-0 crutch, and the B1–B4 / M1–M2 gap analysis behind section 6
- [docs/platform-migration-plan.md](../docs/platform-migration-plan.md) — milestones M0–M5, the non-goals, and the `target_os()` decision
- [docs/platform-support.md](../docs/platform-support.md) — the B1–B4 / S1–S3 survey, the Tier-2 fixes, and the fixed point's Windows-only remainder
- [docs/web-platform-plan.md](../docs/web-platform-plan.md) — decisions A / A2 / B1, milestones W0–W4 and the open-decision list
- [docs/web-benchmark.md](../docs/web-benchmark.md) — the benchmark methodology behind the W-row claims
- [web/README.md](../web/README.md), [web/http_buf.ax](../web/http_buf.ax) — the shipped web suite (W0)
- [tests/pipeline.rs](../tests/pipeline.rs) — the 97 tests, including the fixed point, the `for` tests and `rejects_top_level_statement`
- [src/lexer.rs](../src/lexer.rs), [src/parser.rs](../src/parser.rs) — `Tok::For` / `Stmt::For` and the `import` string-path grammar
- [src/typecheck.rs](../src/typecheck.rs) — the indexing rule that blocks string indexing, and the monomorphizer
- [src/codegen.rs](../src/codegen.rs) — optimization levels, `str(float)`, aggregate ABI
- [src/platform.rs](../src/platform.rs) — extensions, stack flag, `target_os_name`, `llvm_link_name`
- [src/main.rs](../src/main.rs) — the CLI semantics still to be ported, and the content-hash cache
- [stdlib/stdlib.ax](../stdlib/stdlib.ax) — what the standard library already ships
- [selfhost/driver.ax](../selfhost/driver.ax), [selfhost/driver_self_demo.ax](../selfhost/driver_self_demo.ax) — the driver, the fixed-point demo and its hardcoded link name
- Probes run against v0.26.3 (`target/release/aoxn.exe`): string indexing, `char`, `enum`, `match`, top-level statements, bare `import`, `{x:.2f}` / `{x!r}` / `{x:@@@}`, a multi-line f-string, and an `extern def` with a `string` parameter
