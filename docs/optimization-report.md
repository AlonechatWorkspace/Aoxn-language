# Aoxn 优化空间调查报告（重点：编译速度）

- 日期：2026-09-27 ｜ 基线：v0.25.0 + 工作区中已实现的 M0 平台抽象（`src/platform.rs`，Windows 行为不变）
- 方法：release 构建实测（`AOXN_TIME=1` 阶段计时 + 端到端循环计时，数据为多次取样均值/区间）；对照组实验（O1/O2/O3 管线、lld、直连 link.exe、debug 构建）。所有实验在本机 Windows x64（LLVM 23.1.0，lld-link 为默认链接器）。
- 两个代表负载：**小程序** `hello.ax`（10 行）；**大输入** `driver_self_demo.ax`（整个自举编译器，7 个文件约 7k 行）。

## 一、结论（TL;DR）

编译器的**前端已经很薄**（v0.23 的 A1–A5 优化后，7k 行源码 lex+parse+typecheck 共约 32ms，占总耗时 <1%）。编译时间几乎全部花在三处与自身代码无关的固定环节：

| 负载 | 端到端 | 构成（占比） |
|---|---|---|
| hello（10 行） | ~0.5–0.85s | **链接 471ms** + 进程启动 ~127ms（LLVM-C.dll 73MB 加载）+ 编译本体 18ms |
| driver_self_demo（7k 行） | ~5.8s | **O3 优化管线 2.42s（42%）** + **指令选择 2.36s（41%）** + 链接 0.70s（12%）+ 其余 ~2% |

对应的优化空间排序：**① O1 管线选项**（大输入编译时间 −50%，fib 实测运行时 −38% 为代价，循环型代码零损失）；**② `--O0` 真降级**（现在 O0 仍用 CodeGenOptLevel=2，应改为 0 走 fast-isel）；**③ `aoxn run` 构建缓存**（开发循环 0.5–0.85s → 暖缓存 ~10–50ms）；**④ dev profile 提速**（debug 构建下编译器自身慢 8.3 倍）。链接层已是 lld-link 默认，压榨空间有限（≤130ms/次）。

## 二、实测数据

### 2.1 阶段计时（release，`AOXN_TIME=1`）

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

### 2.2 对照实验

| 实验 | 结果 | 含义 |
|---|---|---|
| `default<O2>` vs `default<O3>`（7k 行） | passes 均为 **2.21s**，零差异 | 换 O2 不省时间（瓶颈不在 O3 独有的 pass 上） |
| `default<O1>`（7k 行） | passes **1.13s（−49%）** | 编译时间近乎减半 |
| O1 vs O3 运行时（primes，循环型） | 88ms vs 92ms/次，打平 | 循环型代码无损失 |
| O1 vs O3 运行时（fib，递归调用型） | 184ms vs 133ms/次，**O1 慢 38%** | 内联在调用密集代码上有实质收益 |
| `--O0`（7k 行） | codegen 1.80s（跳过 passes，isel 仍 ~1.7s） | O0 未走 fast-isel（见 F2） |
| 纯启动（`aoxn --help`）×3 | **127ms/次** | LLVM-C.dll 73MB 加载 + CRT 初始化 |
| clang 驱动器启动（`clang --version`）×3 | 136ms/次 | 每次链接重复做 MSVC 检测 |
| 微型 obj 链接（经 clang）×3 | 234ms/次 | 链接成本下限：驱动层 + lld-link + CRT |
| 微型 obj 直连 lld-link ×3 | 255ms/次 | 与经 clang 在噪声内打平，绕过驱动层**无净收益** |
| `-fuse-ld=lld` | 340ms/次 | 坑：解析到 ELF 版 `lld.exe` 而非 `lld-link`，更慢（clang 23 默认已是 lld-link，无需此开关） |
| debug 构建（hello） | codegen 147.8ms vs 17.7ms（**8.3×**） | `cargo run` 开发循环的最大隐性成本 |

## 三、优化机会（按性价比排序）

### F1. 暴露 `--O1` 快速编译档（大输入 −50% 编译时间）
`AOXN_PASSES` 环境变量已支持任意管线（`codegen.rs:592`），但只是一个实验性后门。建议加正式 CLI 旗标 `--O1`（映射 `default<O1>`），并在 `--help` 标注适用场景：**迭代调试、对编译时间敏感的 CI**。默认档保持 O3 不变——"与 clang -O3 同性能"是项目承诺，fib 实测 O1 运行时 −38% 证明内联差异真实存在，但循环型代码（primes）零损失。改动量：CLI 解析 + 一个常量，半小时级。

### F2. `--O0` 时 TargetMachine 降级到 CodeGenOptLevel None（一行改动）
`codegen.rs:401`：`opt=false` 时传的是 `CODEGEN_LEVEL_DEFAULT=2`（`llvm.rs:153`），而 LLVM 的 O0 快速路径（fast-isel + 无优化的寄存器分配）要求 level **0**。当前 `--O0` 只省掉了 IR 管线，isel 仍以 2 级跑（7k 行实测 1.7s）。改为 0 后预期 isel 数倍下降（待实测确认），对 `aoxn run --O0` 的开发循环和测试套件是直接收益。

### F3. `aoxn run` 构建缓存（开发循环 10–50× 加速）
对小程序，端到端 ~0.5–0.85s 里真正的编译只有 18ms，其余全是链接和进程开销——而 `aoxn run` 的典型场景是**同一程序反复改一行再跑**。按（源文件+imports 的内容哈希 + 编译器 mtime + 编译选项）做 exe 缓存（存 `target/cache/`），命中时跳过编译+链接。暖命中 ~10–50ms，比任何链接器优化都高一个量级。实现量中等（哈希遍历 + 缓存失效语义），无 LLVM 依赖。

### F4. dev 构建 profile 提速（一行 Cargo.toml）
`[profile.dev] opt-level = 1`：编译器自身 debug 构建（`cargo run` 的默认）下 codegen 慢 8.3 倍（147.8ms vs 17.7ms），opt-level=1 通常能收回大半。另建议文档注明基准测试一律用 `--release`。

### F5. 链接层：已接近地板，只建议微调
clang 23 默认链接器已是 lld-link，微型 obj 的链接成本 ~230–250ms/次（进程 spawn × 2 层 + CRT 加载），直连 lld-link 无净收益（实测噪声内打平），`-fuse-ld=lld` 反而更慢（选错 flavor）。可做的只有：**缓存工具链探测**省去 clang 驱动层每次 ~136ms 的 MSVC 检测（做法：首次用 `clang -###` 探得 libpath 后，后续直接 spawn lld-link；实测上限 ~130ms/次，属低优先级微调）。若未来上 Linux CI，可评估直接调 `ld.lld` + mold。

### F6. 进程启动 127ms（LLVM-C.dll 73MB）
低价值：只有 `--help`/报错路径能避开（delay-load LLVM-C.dll，Windows 专属改动，`/DELAYLOAD` + delayimp），正常编译路径省不掉。列出备查。

## 四、已核实为非问题（不必再花时间）

- **前端**：7k 行 lex+parse 28.8ms、typecheck 6.1ms——v0.23 的 A1–A5 之后已无低垂果实；
- **cg.build 41–50ms**：FFI 调用本身是下限，`cstr()` 每次 CString 分配等细节合计 <1%；
- **O2 管线**：与 O3 编译时间完全相同，"换 O2 提速"是伪选项；
- **链接器选择**：默认已是 lld-link，换无可换；
- **并行化**：单模块下 LLVM C API 的 passes/isel 均为单线程（结构性限制；真要做需按函数拆模块 + LTO 合并，属长期项）；
- **自举侧**：stage-2 速度由同样的 LLVM 环节主导，上面 F1/F2 同样惠及自举编译产物。

## 五、建议的实施顺序

1. **F2**（一行，先做）→ 2. **F4**（一行）→ 3. **F1**（半天）→ 4. **F3**（1–2 天，收益最大的应用层改动）→ F5/F6 备查。
   全部不触及自举固定点的对比语义（F1/F2 只改默认关闭的可选路径；F3 在编译器外层）。
