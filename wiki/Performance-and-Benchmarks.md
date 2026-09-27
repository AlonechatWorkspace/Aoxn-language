# 性能与基准 · Performance and Benchmarks

> **中文**：Aoxn 的性能承诺是默认走 LLVM `default<O3>`、与 `clang -O3` 在同算法同机上持平；本页给出编译耗时结构、优化级别的实测取舍、内容哈希构建缓存、链接与启动地板，以及本仓库的测量纪律。
> **English**: Aoxn's performance promise is the LLVM `default<O3>` pipeline by default and parity with `clang -O3` on the same algorithm and machine; this page covers where compile time actually goes, the measured optimization-level tradeoffs, the content-hash build cache, the link and startup floor, and this repository's measurement discipline.

## 中文

版本基线 **v0.26.3**，`tests/pipeline.rs` 中 **97 个端到端测试**，CLI 二进制名 **`aoxn`**。
本页只讲**语义与取舍**：`--O0`…`--O3`、缓存与环境变量的完整旗标清单见
[命令行与工具链](CLI-and-Tooling.md)。服务器侧的吞吐/延迟数字见 [Web 平台](Web-Platform.md)。

### 1. 性能承诺的准确表述

承诺由两部分组成，必须一起引用：

- **默认管线**是 LLVM `default<O3>`，`aoxn build` / `aoxn run` 都不改默认档；
  `AOXN_PASSES=<pipeline>` 只是在 level > 0 时覆盖管线文本的实验后门。
- **README 的原话**是 "LLVM O3 backend; measured at parity with `clang -O3`"，
  依据是 README §Performance 的三行实测表（**历史数据**，下表原文照录）：

| Benchmark | Scale | Aoxn | clang C++ |
|---|---|---|---|
| Loop sum | 2×10⁸ iterations | ~15 ms | ~23 ms |
| Array fill + scan | 2×10⁸ reads | 86 ms | 99 ms |
| Struct copies (by value) | 7.5×10⁷ copies | 237 ms | 206 ms |

引用这张表必须同时带上它的限定条件——有些写在 README 表头，有些没写：

- **同算法，不是同一份代码**：对照程序是等价的 C++ 实现，不是同一份源码；
  "parity" 指的是同算法下的可比结果，不是逐条指令等价。
- **同机**：两侧在同一台开发机上编译并运行。README 未标机器型号；
  `docs/optimization-report.md` 的测量环境写作"本机 Windows x64，LLVM 23.1.0，
  lld-link 为默认链接器"（未写 CPU 型号）；`docs/web-benchmark.md` 给出 web 基准所在
  开发机的型号为 **Intel Core i5-1135G7 (4C/8T)**（两份文档都没有声明彼此同机）。
  跨机器复现时请重新测量。
- **热运行 + best-of-3**：**没有误差棒，也没有冷启动数据**。`Struct copies` 一行
  Aoxn 比 clang 慢（237 ms vs 206 ms），说明"持平"是**同数量级/噪声内**的意思，
  不是每一项都更快。
- 基准源在 `examples/`：`bench_array.ax`（2×10⁸ 次读）、`bench_struct.ax`
  （7.5×10⁷ 次按值拷贝）、`bench_for.ax`、`bench_string.ax`、`benchmark.ax`。
  口径提醒：README 把 Loop sum 一行写作 `2×10⁸ iterations`，而
  `examples/benchmark.ax` 的循环上界是 `while i < 100000000`（即 1×10⁸），
  文件注释写 "200M loop iterations"——两处不一致，引用时请注明出处。
- **README 的 Status / 测试数段落已过时**（仍写 "v0.7 · 70/70 tests"），
  但上面这张性能表作为**历史数据**仍可引用；若新旧数据冲突，
  **以 `docs/` 下较新的报告为准**，并在引用处写明各自对应的版本。

如果要用 Web 基准当作"运行时性能"的证据，还要记住那组数字是**压测客户端与服务端
同机**（127.0.0.1）测出来的：Aoxn 侧 ~7.6k req/s 是**压测端上限**而不是服务端上限。
完整表格、Little's law 分析与限定条件见 [Web 平台](Web-Platform.md)。

### 2. 编译耗时花在哪里（前端 < 1%）

来源：`docs/optimization-report.md` §一与 §2.1。环境：本机 Windows x64、LLVM 23.1.0、
lld-link 默认链接器；**基线 v0.25.0**（加上当时工作区里已实现的 M0 平台抽象），
release 构建，方法为 `AOXN_TIME=1` 阶段计时 + 端到端循环计时，数值取多次抽样。两个代表负载：
`examples/hello.ax`（10 行"小程序"）与 `selfhost/driver_self_demo.ax`
（整个自举编译器，7 个文件约 7k 行的"大输入"）。

| 阶段 | hello | 7k 行 driver_self_demo |
|---|---|---|
| lex（全部文件） | 53µs | ~11.3ms |
| parse（全部文件） | 94µs | ~17.5ms |
| typecheck | 64µs | 6.1ms |
| cg.build（IR 构建） | 0.25ms | 41–50ms |
| cg.verify | 0.29ms | 13.1ms |
| cg.passes（O3 管线） | 6.7ms | **2420ms** |
| cg.isel（目标码发射） | 8.1ms | **2356ms** |
| link（clang 子进程） | **471ms** | 702ms |

同一报告的端到端构成：

| 负载 | 端到端 | 构成（占比） |
|---|---|---|
| hello（10 行） | ~0.5–0.85s | **链接 471ms** + 进程启动 ~127ms（LLVM-C.dll 73MB 加载）+ 编译本体 18ms |
| driver_self_demo（7k 行） | ~5.8s | **O3 管线 2.42s（42%）** + **指令选择 2.36s（41%）** + 链接 0.70s（12%）+ 其余 ~2% |

读法（报告的 TL;DR）：

- **前端已经很薄**：7k 行源码 lex+parse+typecheck 共约 **32ms**，占总耗时 **< 1%**
  （报告 §四 另给分解：lex+parse 28.8ms、typecheck 6.1ms）；
- 编译时间几乎全在三个**与自身代码规模无关**的固定环节：**LLVM 的 passes + isel**、
  **链接**、**进程启动**；
- 所以优化顺序是"O1 档 → 真 O0 → 构建缓存 → dev profile"，而不是继续抠前端；
- 报告 §四 另把几条已核实为非问题的项记录在案：前端（v0.23 的 A1–A5 之后无低垂果实）、
  `cg.build` 的 41–50ms（FFI 调用本身是下限）、O2 管线（见下节）、链接器选择、
  **并行化**（单模块下 LLVM C API 的 passes/isel 均为单线程，属结构性限制；
  真要并行需按函数拆模块 + LTO 合并，属长期项）、以及自举侧（同样的 LLVM 环节主导）。

### 3. 优化级别的实测取舍

级别语义（v0.26.2 起，完整旗标清单见 [命令行与工具链](CLI-and-Tooling.md)）：

- `--O0`：跳过 `LLVMRunPasses`，并且 TargetMachine 以 `LLVMCodeGenLevelNone` 创建，
  指令选择走 LLVM 的 **fast-isel** 路径；
- `--O1`：跑 `default<O1>` 管线；
- `--O2` / `--O3`：为对称性都可给，但**最多一个级别旗标**（多给则 exit code 2），默认 O3；
- `AOXN_PASSES=<pipeline>` 在任意 level > 0 时覆盖管线文本。

实测（除注明外均为 7k 行 `driver_self_demo.ax`，来源：`docs/optimization-report.md` §2.2
与 §一，以及 `CHANGELOG.md` v0.26.2）：

| 实验 | 结果 | 含义 |
|---|---|---|
| `default<O2>` vs `default<O3>` | passes 均为 **2.21s**，零差异 | "换 O2 提速"是**伪选项**：瓶颈不在 O3 独有的 pass 上 |
| `default<O1>` | passes **1.13s（−49%）** | 大输入编译时间近乎减半 |
| `--O1`（v0.26.2 复测，自举输入） | passes ~2.4s → **~1.1s（≈ −50%）** | CHANGELOG 的正式记录；推荐用于迭代与对编译时间敏感的 CI |
| O1 vs O3 运行时（primes，循环型） | 88 ms vs 92 ms / 次，**打平** | 循环型代码零损失 |
| O1 vs O3 运行时（fib，递归调用型） | 184 ms vs 133 ms / 次，**O1 慢 38%** | 内联在调用密集代码上有实质收益 |
| `--O0`（7k 行，v0.25.0 基线） | codegen **1.80s**（跳过 passes，isel 仍 ~1.7s） | 当时 O0 **未**走 fast-isel |

关于最后一行有个时点问题必须说明：报告里 `--O0` 的 1.80s 是 **v0.25.0 基线**的数字，
当时 `--O0` 只省掉了 IR 管线，TargetMachine 仍以 `CodeGenOptLevel = 2` 跑指令选择
（报告把它列为待办 F2，"预期 isel 数倍下降（待实测确认）"）。**v0.26.2 已落地 F2**：
`--O0` 现在真正用 `LLVMCodeGenLevelNone`，指令选择走 fast-isel
（`CODEGEN_LEVEL_NONE` / `CODEGEN_LEVEL_LESS` 加进了 `src/llvm.rs`）。
因此"O0 的 isel 仍 ~1.7s"描述的是**修复前**的行为，不代表当前版本；
修复后的 O0 isel 下降幅度报告里**没有给出实测数字**，不要替它编一个。

级别语义受测试约束：`optimization_levels_agree_on_program_output`（O0…O3 产出的程序行为
一致）与 `optimization_levels_produce_distinct_ir`（O0/O1/O2/O3 确实选中不同管线）
在 v0.26.2 加入测试套件（93 → 97 个测试）。

取舍建议（这是语义层判断，不是实测结论）：**迭代调试与编译时间敏感的 CI 用 `--O1`；
交付性能敏感的产物保持默认 O3**（fib 的 −38% 证明内联差异真实存在）；`--O0` 用于最快的
编译—运行循环，不要用它做性能评测。默认档保持 O3 不变，因为"与 `clang -O3` 同性能"是
项目承诺，而 F1/F2 只增加**默认关闭**的可选路径，不触及自举固定点的对比语义。

### 4. 编译缓存：`run` 与 `build` 共享

来源：`CHANGELOG.md` v0.26.2（F3）与 v0.26.3，以及 `README.md`。
`aoxn run` 与 `aoxn build` 共用同一份内容哈希缓存（v0.26.3 起 `build` 也走缓存，
所以同一程序先 `run` 再 `build` 会命中同一条目）：

- **缓存键**：入口文件 + **全部传递导入**的内容哈希，加上**编译器身份**
  （二进制 size + mtime）与**所有影响代码生成的选项**（优化级别、`AOXN_CPU`、
  `AOXN_PASSES`、`-l`/`-L`、解析到的 clang 路径）。任何一项变化即 miss。
- **命中时**：跳过编译与链接。`run` 直接跑缓存的可执行文件；`build` 把缓存 exe
  拷贝到 `-o` 目标。
- **实测暖命中量级**（开发机，见下表的出处）：
  `Aoxn run examples/hello.ax` **1113ms（冷）→ ~85ms（暖）**；
  重复 `aoxn build examples/hello.ax -o out.exe` **~0.8–1.6s → ~0.2s**。
  报告对 `run` 的预期量级是暖命中 **~10–50ms**（小程序端到端 ~0.5–0.85s 里真正的编译
  只有 18ms，其余全是链接与进程开销）。
- **容量与淘汰**：绑定 **64 条**，近似 LRU（oldest-first，命中会刷新 mtime）。
- **开关**：`AOXN_NO_CACHE=1` 关闭；`AOXN_CACHE_DIR=<dir>` 迁移缓存目录
  （默认在 `target/cache`）。并发调用通过同一个 `<key>.<pid>` 临时文件 + rename 发布。
- **语义不变**：缓存只在完全命中时跳过工作，失效即全量编译 + 链接；它不改变生成的代码。
  辅助函数 `aoxn::dependency_files` 返回入口文件加全部传递导入（文件不可读时返回 `None`，
  此时缓存被禁用）。

### 5. 链接与启动地板（F5 已被数据否决并关闭）

这一节的结论是"**这里没有可赚的钱**"，请把它当**已关闭**的议题读，不要当成待办优化。

| 探针 | 实测 | 含义 |
|---|---|---|
| `aoxn --help` ×3（纯启动） | **127ms / 次** | LLVM-C.dll 73MB 加载 + CRT 初始化，正常编译路径省不掉 |
| `clang --version` ×3（报告早期） | 136ms / 次 | clang 驱动每次重复做 MSVC 检测 |
| `clang --version`（v0.26.3 复核） | **156–189ms**，设不设 `INCLUDE`/`LIB` **完全一样** | clang 23.1.0 **没有** env 快速路径 |
| 微型 obj 经 clang 链接 ×3 | **234ms / 次** | 链接成本下限：驱动层 + lld-link + CRT |
| 微型 obj 直连 lld-link ×3 | 255ms / 次 | 与经 clang 在噪声内打平，**绕过驱动层无净收益** |
| `-fuse-ld=lld` | 340ms / 次 | 坑：解析到 ELF 版 `lld.exe` 而非 `lld-link`，**更慢**（clang 23 默认已是 lld-link） |
| 微型 obj 链接（v0.26.3 复核，中位数） | 667ms（无 env）/ 645ms（设 env） | 与上面的 234ms 来自**不同时段**，见本节末的说明 |

- **F5 原设想**是"缓存工具链探测，省掉 clang 每次 ~136ms 的 MSVC 检测"。
  两项做法都被数据否决：环境变量不改变驱动启动成本；直连 lld-link 无净收益。
  结论是 **F5 从"备查"改判为"不可行"**，剩余成本是 DLL 加载地板
  （本编译器的 LLVM-C.dll ~127ms、clang 自身的 DLL 链），属结构性下限。
- **F6（delay-load LLVM-C.dll）** 属低价值、Windows 专属改动，只对 `--help`/报错路径有效，
  正常编译路径省不掉——列在报告里备查，未实施。
- 关于 234ms 与 667ms 的差异：报告在 §六 明确写下"开发机 10–50ms 级相位的标准差可达
  **±2×**（杀毒/索引服务）"，两组数字取自已相隔一段时间的不同测量时段。
  **这正是本仓库要求交替多次取样、并禁止跨日比较的原因**——两组数字不可混用，
  也不表示回归。

### 6. 开发构建提示

- **`[profile.dev] opt-level = 1`**（`Cargo.toml`，v0.26.2 落地 F4）：
  编译器自身在**未优化**的 debug 构建下 codegen 慢 **8.3×**
  （实测 147.8ms vs 17.7ms，hello），`cargo run` 的每次迭代都在付这笔钱；
  `opt-level = 1` 通常能收回大半。
- **基准一律用 `--release`**：`CONTRIBUTING.md` 的 PR 规则与报告都这么要求。
- 观测工具：`AOXN_TIME=1` 打印各流水线阶段与 codegen 子阶段
  （`cg.build` / `cg.verify` / `cg.target` / `cg.passes` / `cg.isel`）；
  `AOXN_TC_TRACE=1` / `AOXN_CG_TRACE=1` 打每个函数的阶段标记。
  **优化任何东西之前先测**：

```powershell
cargo build --release                    # 基准永远用 release
$env:AOXN_TIME = "1"                     # 打印各阶段耗时（lex/parse/typecheck/cg.*/link）
cargo run --release -- run selfhost\driver_self_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
$env:AOXN_PASSES = "default<O1>"         # 任意管线文本，level > 0 时生效
```

### 7. 测量方法学（本页最重要的一节）

本仓库自己的规则（`CONTRIBUTING.md`：Performance 一行要求"先测量"，PR 规则第 4 条
要求性能声明必须给出**数字与方法**）与 `docs/optimization-report.md` §六 的注记合起来是：

1. **相位噪声很大**：开发机上 10–50ms 级的相位标准差可达 **±2×**（杀毒软件/索引服务）。
   任何"噪声内"的结论都必须基于**同机交替 5 次取中位数/最小值**。
2. **跨相位对比不构成证据**：报告点名了一个例子——typecheck 3.97ms → 4.08ms
   既不算回归也不算改进。v0.26.3 的前端微优化（单态化队列改 `VecDeque`、
   struct 字段查找 O(1)、`type_size` 记忆化、调用点不再克隆 `params`、
   `AOXN_*_TRACE` 每编译读一次、`printf` 走统一 externs 缓存）**单项收益都在本机噪声
   地板之下**——CHANGELOG 如实标注为"真实性价比较低的卫生改造"，没有虚构百分比。
3. **跨机器或跨日比较无意义**；要比较就在同一台机器、同一时段、交替跑。
4. **报告格式**（`docs/optimization-report.md` 是范例，也是 `CONTRIBUTING.md` 指向的模板）：
   **数字 + 机器 + 方法 + 前后对比**。写性能结论时请照抄这个结构：
   "在什么机器上、用什么方法、测到什么数字、与什么基线相比"。
5. **性能类 PR 需要给出数字与方法**（machine、warm/cold、best of N），
   基准用 `--release`；没有数字的性能声明在评审里会被要求补测。
6. **写清版本与基线**：本页引用的 `docs/optimization-report.md` 主体数据来自
   **v0.25.0 基线**（`--O0` 1.80s 即为一例），v0.26.2/0.26.3 的 `CHANGELOG.md`
   才是 F1–F4 落地后的记录。引用旧数字时必须带上"哪个版本、何时测的"。

### 8. 本页与 Web 平台页的分工

一句话：**本页讲"编译器有多快、生成代码有多快"**——编译耗时结构、优化级别取舍、
构建缓存、链接/启动地板、原生基准与方法学；**[Web 平台](Web-Platform.md) 讲"用 Aoxn
写的服务器有多快"**——HTTP/1.1 参考服务器、功能测试、CI 与 web 基准的吞吐/延迟/启动/RSS
表格。两页的数字出处不同（本页主要来自 `docs/optimization-report.md` 与 `README.md`，
Web 页主要来自 `docs/web-benchmark.md` 与 `web/loadtest/last-results.json`），
不要交叉混用。

## English

Baseline: **v0.26.3**, **97 end-to-end tests** in `tests/pipeline.rs`, CLI binary **`aoxn`**.
This page covers **semantics and tradeoffs**; the complete flag list for `--O0`…`--O3`,
the cache and the environment variables lives in [CLI and Tooling](CLI-and-Tooling.md).
Throughput/latency numbers for the server side live in [Web Platform](Web-Platform.md).

### 1. What the performance promise actually says

The promise has two halves, and they must be quoted together:

- The **default pipeline** is LLVM `default<O3>`; neither `aoxn build` nor `aoxn run`
  changes the default. `AOXN_PASSES=<pipeline>` is only an experimental back door that
  overrides the pipeline text at any level > 0.
- The **README wording** is "LLVM O3 backend; measured at parity with `clang -O3`",
  backed by the three-row table in README §Performance (**historical data**, reproduced
  verbatim below):

| Benchmark | Scale | Aoxn | clang C++ |
|---|---|---|---|
| Loop sum | 2×10⁸ iterations | ~15 ms | ~23 ms |
| Array fill + scan | 2×10⁸ reads | 86 ms | 99 ms |
| Struct copies (by value) | 7.5×10⁷ copies | 237 ms | 206 ms |

Quoting that table requires quoting its qualifications — some are in the README header,
some are not written down at all:

- **Same algorithm, not the same code**: the comparison is against equivalent C++
  implementations, not against one shared source. "Parity" means comparable results for
  the same algorithm, not instruction-for-instruction equality.
- **Same machine**: both sides are compiled and run on one dev machine. The README does
  not name the machine; `docs/optimization-report.md` records its environment as "this
  machine, Windows x64, LLVM 23.1.0, lld-link as the default linker" (no CPU model),
  while `docs/web-benchmark.md` gives the dev machine behind the web benchmark as an
  **Intel Core i5-1135G7 (4C/8T)** (neither document claims they are the same machine).
  Re-measure before claiming these numbers elsewhere.
- **Warm runs, best of 3**: **there is no error bar and no cold-start data.** In the
  `Struct copies` row Aoxn is *slower* than clang (237 ms vs 206 ms), which is exactly
  what "parity" means here — the same order of magnitude / within noise, not faster in
  every row.
- The benchmark sources are in `examples/`: `bench_array.ax` (2×10⁸ reads),
  `bench_struct.ax` (7.5×10⁷ by-value copies), `bench_for.ax`, `bench_string.ax`,
  `benchmark.ax`. A caveat on scale: the README labels the Loop sum row as
  `2×10⁸ iterations`, but `examples/benchmark.ax` loops `while i < 100000000` (1×10⁸)
  and its own comment says "200M loop iterations" — the two disagree, so cite the source
  you are quoting.
- **The README Status / test-count section is stale** (it still says "v0.7 · 70/70
  tests"), but the performance table above may still be cited as **historical data**.
  When old and new numbers conflict, **the newer report under `docs/` wins** — state
  which version each number belongs to.

If the web benchmark is used as evidence about *runtime* performance, remember those
numbers were measured with the **load client and the servers on the same machine**
(127.0.0.1): Aoxn's ~7.6k req/s is a **load-generator ceiling**, not the server's ceiling.
The full tables, the Little's-law analysis and the caveats are in
[Web Platform](Web-Platform.md).

### 2. Where compile time goes (front end < 1%)

Source: `docs/optimization-report.md` §1 and §2.1. Environment: this machine, Windows x64,
LLVM 23.1.0, lld-link as the default linker; **baseline v0.25.0** (plus the M0 platform
abstraction already in the working tree at the time), release build, method =
`AOXN_TIME=1` stage timing plus end-to-end loop timing, values averaged over several
samples. Two representative loads: `examples/hello.ax` (a 10-line "small program") and
`selfhost/driver_self_demo.ax` (the whole self-hosting compiler, seven files, ~7k lines —
the "large input").

| Stage | hello | 7k-line driver_self_demo |
|---|---|---|
| lex (all files) | 53µs | ~11.3ms |
| parse (all files) | 94µs | ~17.5ms |
| typecheck | 64µs | 6.1ms |
| cg.build (IR construction) | 0.25ms | 41–50ms |
| cg.verify | 0.29ms | 13.1ms |
| cg.passes (O3 pipeline) | 6.7ms | **2420ms** |
| cg.isel (target emission) | 8.1ms | **2356ms** |
| link (clang subprocess) | **471ms** | 702ms |

The same report's end-to-end breakdown:

| Load | End to end | Composition (share) |
|---|---|---|
| hello (10 lines) | ~0.5–0.85s | **link 471ms** + process startup ~127ms (loading the 73MB LLVM-C.dll) + compilation proper 18ms |
| driver_self_demo (7k lines) | ~5.8s | **O3 pipeline 2.42s (42%)** + **instruction selection 2.36s (41%)** + link 0.70s (12%) + the rest ~2% |

How to read it (the report's TL;DR):

- **The front end is already thin**: lex + parse + typecheck over 7k lines total about
  **32ms**, **< 1%** of the wall clock (the report's §4 gives the split: lex+parse 28.8ms,
  typecheck 6.1ms).
- Almost all compile time sits in three **fixed costs that do not scale with your own
  source**: **LLVM's passes + isel**, **linking**, and **process startup**.
- Hence the optimization order is "O1 level → real O0 → build cache → dev profile", not
  more front-end micro-tuning.
- The report's §4 also records what was verified as *not* actionable: the front end (no
  low-hanging fruit left after v0.23's A1–A5), `cg.build`'s 41–50ms (the FFI calls
  themselves are the floor), the O2 pipeline (see the next section), linker choice,
  **parallelization** (with a single module, LLVM's C-API passes/isel are single-threaded
  — a structural limit; real parallelism needs per-function modules plus LTO, a long-term
  item), and the self-hosted side (dominated by the same LLVM stages).

### 3. Measured optimization-level tradeoffs

Level semantics (as of v0.26.2; the full flag list is in
[CLI and Tooling](CLI-and-Tooling.md)):

- `--O0`: skips `LLVMRunPasses` **and** creates the target machine with
  `LLVMCodeGenLevelNone`, so instruction selection takes LLVM's **fast-isel** path;
- `--O1`: runs the `default<O1>` pipeline;
- `--O2` / `--O3`: both accepted for symmetry, but **at most one level flag** (more exits
  with code 2); the default is O3;
- `AOXN_PASSES=<pipeline>` overrides the pipeline text at any level > 0.

Measurements (all on the 7k-line `driver_self_demo.ax` unless noted; sources:
`docs/optimization-report.md` §2.2 and §1, plus `CHANGELOG.md` v0.26.2):

| Experiment | Result | Meaning |
|---|---|---|
| `default<O2>` vs `default<O3>` | passes both **2.21s**, zero difference | "Switch to O2 for speed" is a **pseudo-option**: the bottleneck is not an O3-only pass |
| `default<O1>` | passes **1.13s (−49%)** | compile time on large inputs nearly halved |
| `--O1` (re-measured in v0.26.2, self-host input) | passes ~2.4s → **~1.1s (≈ −50%)** | the changelog's official record; recommended for iteration and compile-time-sensitive CI |
| O1 vs O3 runtime (primes, loop-shaped) | 88 ms vs 92 ms per run, **a tie** | loop-shaped code loses nothing |
| O1 vs O3 runtime (fib, recursion-heavy) | 184 ms vs 133 ms per run, **O1 38% slower** | inlining earns real money on call-dense code |
| `--O0` (7k lines, v0.25.0 baseline) | codegen **1.80s** (passes skipped, isel still ~1.7s) | at that time O0 did **not** use fast-isel |

One timing caveat is essential for that last row: the 1.80s `--O0` figure belongs to the
**v0.25.0 baseline**, when `--O0` only skipped the IR pipeline while the target machine
still ran instruction selection at `CodeGenOptLevel = 2` (the report lists this as pending
item F2, "expected: isel drops several-fold — to be confirmed by measurement").
**v0.26.2 landed F2**: `--O0` now really uses `LLVMCodeGenLevelNone` and takes the
fast-isel path (`CODEGEN_LEVEL_NONE` / `CODEGEN_LEVEL_LESS` were added to `src/llvm.rs`).
So "O0's isel is still ~1.7s" describes **pre-fix** behaviour, not the current release;
the report gives **no measured post-fix number** for O0 isel — do not invent one.

The level semantics are pinned by tests: `optimization_levels_agree_on_program_output`
(O0…O3 produce identical program behaviour) and `optimization_levels_produce_distinct_ir`
(O0/O1/O2/O3 really select different pipelines) joined the suite in v0.26.2
(93 → 97 tests).

The tradeoff advice (a semantics-level judgement, not a measurement): **use `--O1` for
iteration and compile-time-sensitive CI; keep the default O3 for performance-sensitive
deliverables** (fib's −38% proves the inlining difference is real); use `--O0` for the
fastest compile-and-run loop, never for performance evaluation. The default stays O3
because "parity with `clang -O3`" is a project promise, and F1/F2 only add **opt-in,
default-off** paths that do not touch the comparison semantics of the self-hosting fixed
point.

### 4. The build cache: shared by `run` and `build`

Sources: `CHANGELOG.md` v0.26.2 (F3) and v0.26.3, plus `README.md`.
`aoxn run` and `aoxn build` share one content-hash cache (since v0.26.3 `build` goes
through it too, so running and then building the same program hits the same entry):

- **Cache key**: content hash of the entry file **plus all transitive imports**, plus the
  **compiler's identity** (binary size + mtime) and **every codegen-affecting option**
  (optimization level, `AOXN_CPU`, `AOXN_PASSES`, `-l`/`-L`, the resolved clang path).
  Any change is a miss.
- **On a hit**: compilation and linking are skipped. `run` executes the cached
  executable; `build` copies the cached exe to the `-o` target.
- **Measured warm-hit magnitude** (dev machine; see the sources for each):
  `Aoxn run examples/hello.ax` **1113ms cold → ~85ms warm**; a repeated
  `aoxn build examples/hello.ax -o out.exe` **~0.8–1.6s → ~0.2s**. The report's expected
  magnitude for `run` is a warm hit in the **~10–50ms** range (of the ~0.5–0.85s
  end-to-end for a small program, only 18ms is actual compilation — the rest is linking
  and process overhead).
- **Capacity and eviction**: bounded to **64 entries**, approximate LRU (oldest-first;
  a hit refreshes the mtime).
- **Switches**: `AOXN_NO_CACHE=1` disables it; `AOXN_CACHE_DIR=<dir>` relocates it
  (default: `target/cache`). Concurrent invocations publish through the same
  `<key>.<pid>` temp file plus rename.
- **Semantics are unchanged**: the cache only skips work on a full hit; a miss means a
  full compile + link, and the generated code never depends on it. The helper
  `aoxn::dependency_files` returns the entry file plus every transitive import (it returns
  `None` for an unreadable file, which disables the cache).

### 5. The link and startup floor (F5 was closed by data)

The conclusion of this section is "**there is no money left here**" — read it as a
**closed** topic, not as a pending optimization.

| Probe | Measurement | Meaning |
|---|---|---|
| `aoxn --help` ×3 (pure startup) | **127ms per call** | loading the 73MB LLVM-C.dll + CRT init; the normal compile path cannot avoid it |
| `clang --version` ×3 (early report) | 136ms per call | the clang driver redoes MSVC detection on every link |
| `clang --version` (v0.26.3 re-check) | **156–189ms**, **identical** with and without `INCLUDE`/`LIB` | clang 23.1.0 has **no** environment fast path |
| tiny obj linked through clang ×3 | **234ms per call** | the link cost floor: driver + lld-link + CRT |
| tiny obj linked by spawning lld-link directly ×3 | 255ms per call | a wash with the clang route within noise; **bypassing the driver buys nothing** |
| `-fuse-ld=lld` | 340ms per call | trap: resolves to the ELF-flavoured `lld.exe`, not `lld-link`, and is **slower** (clang 23 already defaults to lld-link) |
| tiny obj link (v0.26.3 re-check, median) | 667ms (no env) / 645ms (env set) | a **different session** than the 234ms above — see the note at the end of this section |

- **The original F5 idea** was to cache toolchain probing and save clang's ~136ms MSVC
  detection. Both variants were rejected by data: environment variables do not change
  driver startup, and spawning lld-link directly buys nothing. The verdict is that
  **F5 moved from "parked" to "not feasible"**; the remaining cost is a DLL-load floor
  (LLVM-C.dll ~127ms for this compiler, plus clang's own DLL chain), i.e. a structural
  minimum.
- **F6 (delay-loading LLVM-C.dll)** is low-value and Windows-specific: it only helps the
  `--help`/error paths, never a normal compile. It is listed in the report, not
  implemented.
- About the 234ms vs 667ms gap: the report states in §6 that on this dev machine the
  standard deviation of 10–50ms phases reaches **±2×** (anti-virus/indexer). The two sets
  of numbers come from measurement sessions separated in time. **This is exactly why this
  repository requires interleaved repeated sampling and forbids cross-day comparisons** —
  the numbers must not be mixed, and the gap is not a regression.

### 6. Development-build notes

- **`[profile.dev] opt-level = 1`** (`Cargo.toml`, F4 in v0.26.2): built unoptimized in
  debug mode, the compiler's own codegen is **8.3× slower** (147.8ms vs 17.7ms measured on
  hello), and every `cargo run` iteration pays for it; `opt-level = 1` usually recovers
  most of that.
- **Always benchmark with `--release`**: both `CONTRIBUTING.md` and the report say so.
- Instrumentation: `AOXN_TIME=1` prints every pipeline stage plus the codegen sub-stages
  (`cg.build` / `cg.verify` / `cg.target` / `cg.passes` / `cg.isel`);
  `AOXN_TC_TRACE=1` / `AOXN_CG_TRACE=1` print per-function stage markers.
  **Measure before optimizing anything**:

```powershell
cargo build --release                    # always benchmark with release
$env:AOXN_TIME = "1"                     # per-stage timings (lex/parse/typecheck/cg.*/link)
cargo run --release -- run selfhost\driver_self_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
$env:AOXN_PASSES = "default<O1>"         # arbitrary pipeline text, active at any level > 0
```

### 7. Measurement discipline (the most important section on this page)

This repository's own rules — the "Performance" row in `CONTRIBUTING.md` ("measure first",
with `docs/optimization-report.md` as the format model) and PR rule 4 (performance claims
need numbers and a method) — combined with the notes in `docs/optimization-report.md` §6,
come down to this:

1. **Phase noise is large**: on the dev machine, 10–50ms phases swing by up to **±2×**
   (anti-virus, indexing). Any "within noise" conclusion must rest on **interleaved runs
   on the same machine — 5 samples, median/minimum**.
2. **Cross-phase comparisons are not evidence**: the report names an example — typecheck
   moving from 3.97ms to 4.08ms is neither a regression nor an improvement. v0.26.3's
   front-end micro-optimizations (the monomorphization queue becoming a `VecDeque`, O(1)
   struct-field lookup, `type_size` memoization, no more `params` clones at call sites,
   `AOXN_*_TRACE` read once per compile, `printf` going through the shared externs cache)
   are each **below this machine's noise floor** — the changelog labels them honestly as
   hygiene work with a low truth-to-value ratio instead of inventing percentages.
3. **Cross-machine or cross-day comparisons are meaningless**: compare on one machine, in
   one session, interleaved.
4. **Report format** (`docs/optimization-report.md` is the model, and the template
   `CONTRIBUTING.md` points at): **numbers + machine + method + before/after**. Write
   performance conclusions in that shape: on which machine, by which method, what numbers,
   against which baseline.
5. **Performance PRs need numbers and a method** (machine, warm/cold, best of N) and must
   benchmark with `--release`; a performance claim without numbers will be asked to
   measure again in review.
6. **State the version and the baseline**: most of the data on this page comes from the
   `docs/optimization-report.md` **v0.25.0 baseline** (the 1.80s `--O0` figure is one
   example); the records of what F1–F4 actually landed are in `CHANGELOG.md`
   v0.26.2 / v0.26.3. Any old number must be quoted with "which version, measured when".

### 8. How this page and the Web Platform page divide the work

In one sentence: **this page is about how fast the compiler is and how fast the code it
generates is** — compile-time structure, optimization-level tradeoffs, the build cache,
the link/startup floor, native benchmarks and measurement discipline. **[Web Platform](Web-Platform.md)
is about how fast a server written in Aoxn is** — the HTTP/1.1 reference server, its
functional tests, CI, and the throughput/latency/startup/RSS tables of the web benchmark.
The two pages draw on different sources (mainly `docs/optimization-report.md` and
`README.md` here; `docs/web-benchmark.md` and `web/loadtest/last-results.json` there).
Do not mix them.

---

## 源文件 / Source files

- [docs/optimization-report.md](../docs/optimization-report.md) — stage timings, the O1/O2/O3/O0 and link experiments, the F5 re-check, and the measurement notes
- [CHANGELOG.md](../CHANGELOG.md) — v0.26.2 / v0.26.3 entries: F1–F4, the `--O1` and cache numbers, F5 closure
- [README.md](../README.md) — the historical `clang -O3` performance table and the `run` cache note (its Status section is stale)
- [CONTRIBUTING.md](../CONTRIBUTING.md) — "measure first" and the numbers-and-method rule for performance PRs
- [Cargo.toml](../Cargo.toml) — version 0.26.3, `[profile.dev] opt-level = 1`, `[profile.release]`
- [docs/p2-compiler-performance.md](../docs/p2-compiler-performance.md) — the plan that introduced `AOXN_TIME=1` and catalogued the front-end hot spots (A1–A5) and what is explicitly out of scope
- [examples/bench_array.ax](../examples/bench_array.ax) · [examples/bench_struct.ax](../examples/bench_struct.ax) · [examples/bench_for.ax](../examples/bench_for.ax) · [examples/bench_string.ax](../examples/bench_string.ax) · [examples/benchmark.ax](../examples/benchmark.ax) — the benchmark sources behind the README table
- [docs/web-benchmark.md](../docs/web-benchmark.md) — source of the web-side runtime numbers referenced from §1
- [AGENTS.md](../AGENTS.md) — local, gitignored session notes (verified environment facts: LLVM 23.1.0, the ±2× noise floor, F5 closure); not a published repository document
