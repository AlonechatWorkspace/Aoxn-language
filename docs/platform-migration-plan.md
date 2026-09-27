# Aoxn 跨平台迁移实施计划（macOS / Linux）

- 日期：2026-09-27 ｜ 依据：[platform-support.md](platform-support.md)（平台支持调查报告，阻塞项编号 B1–B4、S1–S3 沿用该文）
- 基线版本：v0.25.0（8d8a708）
- 建议：在特性分支 `port/platform` 上按里程碑推进，每个里程碑一个 PR，全部合并后版本号 bump 至 v0.26.0 并发布。

## 0. 目标与非目标

**目标**：Linux x86_64、macOS x86_64（Intel）、macOS arm64（Apple Silicon）三平台上，`aoxn` 编译器完整可用（build/run/ir）、93 项集成测试全绿、自举固定点成立、CI 矩阵覆盖三平台。

**非目标**：交叉编译（`--target` 指定非宿主平台）、MinGW 工具链、32 位平台、官方安装包分发。这些留到平台声明稳定后另行立项。

**总体策略**：先把 Rust 编译器（stage 0）在三个平台打通——这是全部后续工作的地基；自举侧（stage 2）最后对齐，因为它的每一步修改都要重跑固定点验证，成本最高，且依赖一个小的语言设施决策（M4）。

## 1. 里程碑总览

| 里程碑 | 内容 | 估时 | 依赖 |
|---|---|---|---|
| M0 | Rust 侧平台抽象 + build.rs 平台化 | 0.5 天 | — |
| M1 | Linux x86_64 打通 + CI | 0.5–1 天 + CI 调试 | M0 |
| M2 | macOS Intel 打通 + CI | 0.5 天 + CI 调试 | M0 |
| M3 | Apple Silicon（AArch64 后端）+ CI | 0.5 天 + CI 调试 | M0 |
| M4 | 自举侧对齐（含 `target_os()` 语言设施） | 1–2 天 | M1（验收须 M1–M3） |
| M5 | 文档、spec 声明、发布 | 0.5 天 | M1–M4 |

M1/M2/M3 相互独立，可并行（甚至同 PR 添加多个 CI job）；M4 必须在 M1 实测 PIE 结论出来之后动工。

> 本地验证环境提示：开发机是 Windows。Linux 侧建议用 **WSL2 (Ubuntu 24.04)** 本地自测（可跑 cargo + clang，调试效率远高于推送等 CI）；macOS 侧只能依赖 GitHub Actions 的 `macos-13`（Intel）/ `macos-14`（arm64）runner。

## 2. M0 —— Rust 侧平台抽象与 build.rs 平台化

目前平台分支散落在 6 处（`lib.rs:293,308,379`、`main.rs:185,196`、`codegen.rs:469`），先收拢，避免后续每个里程碑各改一遍。

### T0.1 新增 `src/platform.rs`，集中平台判定

```rust
pub fn exe_ext() -> &'static str { if cfg!(windows) { "exe" } else { "" } }
pub fn obj_ext() -> &'static str { if cfg!(windows) { "obj" } else { "o" } }
/// 主线程栈：Windows 默认 1MB，必须用 /STACK 提到 8MB；
/// Linux 主线程栈由 RLIMIT_STACK 决定（主流发行版默认 8MB），链接器选项对主线程无效；
/// macOS 主线程固定 8MB。=> 仅 Windows 需要传链接器标志。
pub fn stack_link_flag() -> Option<&'static str> {
    if cfg!(windows) { Some("-Wl,/STACK:8388608") } else { None }
}
pub fn is_windows() -> bool { cfg!(windows) }
```

`lib.rs::link_opts`、`main.rs::default_exe/temp_exe`、`lib.rs:328,350`（`with_extension("obj")` → `obj_ext()`，Linux 惯例 `.o`）全部改走这里。行为在 Windows 上逐字节不变。

### T0.2 `build.rs` 平台化（解决 B1）

按 `CARGO_CFG_TARGET_OS`（build script 标准环境变量）分支：

| 平台 | 探测候选（依序） | 链接名 |
|---|---|---|
| windows | `AOXN_LLVM_DIR` → `<repo>/LLVM` → `C:\Program Files\LLVM`，判据 `lib/LLVM-C.lib` | `LLVM-C`（现状不变） |
| linux | `AOXN_LLVM_DIR` → `<repo>/LLVM` → `/usr/lib/llvm-XX`（XX=18..=23 倒序）→ `/usr/lib/llvm` | 探测到 `libLLVM-C.so` 用 `LLVM-C`，否则 `libLLVM-XX.so`/`libLLVM.so` 用 `LLVM` |
| macos | `AOXN_LLVM_DIR` → `<repo>/LLVM` → `/opt/homebrew/opt/llvm`（arm64）→ `/usr/local/opt/llvm`（Intel） | `LLVM`（`libLLVM.dylib`） |

非 Windows 的判据改为"`lib/` 下存在上述任一共享库"；同时为运行期加一行 `println!("cargo:rustc-link-arg=-Wl,-rpath,{libdir}")`（Linux；macOS 用 `-Wl,-rpath,{libdir}` 同型），让编译出的 `aoxn` 自身能找到 libLLVM，免设 `LD_LIBRARY_PATH`。找不到时 panic 文案按平台列出已搜索路径与需要的库名。

**顺带项（同 PR）**：`build.rs:6,10` 读的是 `AXON_LLVM_DIR`，而 AGENTS.md/文档写的是 `AOXN_LLVM_DIR`。统一为 `AOXN_LLVM_DIR`，旧名保留一个版本的兼容读取，CHANGELOG 记录。

**验收**：Windows `cargo build/test` 行为不变（现有 93 测试全绿即证明）；WSL2 中 `AOXN_LLVM_DIR=/usr/lib/llvm-18 cargo build` 成功。

## 3. M1 —— Linux x86_64

### T1.1 链接标志（B2）
`lib.rs:379` 改用 `platform::stack_link_flag()`，有则传无则跳过。

### T1.2 产物命名
`main.rs:196` `temp_exe` 的 `format!("{stem}-{}.exe", …)` → `format!("{stem}-{}{}", pid, exe_ext())`。`lib.rs:328,350` 的 obj 命名随 T0.1。

### T1.3 测试套件去硬编码（S3 的 Rust 部分）
`tests/pipeline.rs` 约 40 处 `.exe` 字面量：新增 `fn exe_name(dir: &Path, stem: &str) -> PathBuf`（内部拼 `exe_ext()`），逐处替换（`t{id}.exe`、`out.exe`、`exit-{pid}.exe`、`selfhost-lex-{pid}.exe` 等）。注意 `exe.with_extension("obj")` 的清理与 `lib.rs` 生成端使用同一 `obj_ext()`，天然一致。

### T1.4 PIE 实测（B4 裁决）
在 Ubuntu 上跑覆盖面最全的用例（hello、fib、vectors、stdlib_demo——结构体 sret、大数组、字符串）。两种结果都有预案：

- **通过** → `RELOC_DEFAULT=0` 在 Linux 下被 LLVM 解析为 PIC，结案，不改代码；
- **报 `relocation R_X86_64_32S can not be used … recompile with -fPIE`** → `src/llvm.rs:149` 保持 `RELOC_DEFAULT=0` 不动，在 `codegen.rs:543` 创建 TargetMachine 处按三元组分支：triple 含 `linux`/`darwin` 时传 `RELOC_PIC(2)`。**固定点影响**：reloc 改变只影响 object 字节，不影响 IR 文本；固定点对比是同平台 Rust 编译器 vs 自举编译器，只要 M4 两侧用同一判据，字节一致仍成立。

### T1.5 CI（`.github/workflows/ci.yml`）
增加 `linux` job：`runs-on: ubuntu-latest`，步骤 `apt-get install llvm-18`（或 apt.llvm.org 脚本），`AOXN_LLVM_DIR=/usr/lib/llvm-18 cargo build && cargo test`，smoke 测试沿用（`primes.exe` → `primes`）。缓存 key 与 Windows job 隔离（已有 `runner.os` 前缀，天然隔离）。

**验收**：ubuntu-latest 上 build + 93 测试 + smoke 全绿；`cargo run -- run examples/hello.ax` 输出正确。

## 4. M2 —— macOS Intel

1. build.rs 已按 T0.2 支持 brew 布局；验证 `/usr/local/opt/llvm/lib/libLLVM.dylib`。
2. `find_clang()`（`lib.rs:293-302`）补充 brew 路径候选（keg-only 的 llvm 默认不在 PATH：`/usr/local/opt/llvm/bin/clang`）。
3. 栈标志：不传（T0.1 结论）；若个别大数组用例栈溢出（macOS 主线程 8MB 与测试假设一致，预期不会），用 `ulimit -s` 或 pthread 替代方案，不动链接器。
4. CI 增加 `macos-13` job（`brew install llvm`，同 M1 结构）。

**验收**：macos-13 全绿。

## 5. M3 —— Apple Silicon（aarch64）

1. `src/codegen.rs:17-22` `init_target()` 追加 `LLVMInitializeAArch64{TargetInfo,Target,TargetMC,AsmPrinter}`（`llvm.rs` 加四个 `extern "C"` 声明；符号存在性已在本机 LLVM-C.lib 验证）。注册后 `init_target` 仍保持 Once 语义不变。
2. 无需按三元组条件注册：X86 与 AArch64 同时注册不冲突，`LLVMGetTargetFromTriple` 按三元组选择，与平台无关，代码最简。
3. ABI 回归：AAPCS64 的结构体 sret/传参由 LLVM 处理，现有 93 测试（含结构体、二维数组、泛型、字符串）即回归矩阵，无需新增用例；若有失败按用例定位。
4. CI 增加 `macos-14`（arm64）job。

**验收**：三平台 CI 矩阵全绿；`--cpu native` 在 arm64 上可用（SIMD 特性由 LLVM 自动映射，预期无需改动）。

## 6. M4 —— 自举侧对齐（含一个语言设施决策）

自举侧的两个阻塞（S1/S2）本质相同：**Aoxn 代码没有任何运行时或编译时的平台感知手段**（无 `cfg`、无 `getenv`，已核实 stdlib/selfhost/src 均无）。推荐一次性引入最小设施，同时服务本里程碑和下一阶段（`main.rs` CLI 语义移植）。

### T4.1 新增 builtin：`target_os() -> string`（语言变更，走完整流程）

- **语义**：无参函数，返回 `"windows" | "linux" | "macos" | "other"`，编译期折叠为模块内私有静态字符串常量（值来自编译器宿主三元组解析：含 `windows` → windows，`linux` → linux，`darwin` → macos，否则 other）。不是运行时系统调用。
- **两侧对称实现**（固定点的硬要求）：Rust 侧 `codegen.rs` 把 `cfg!(windows)` 等替换为同一 triple 解析函数（`LLVMGetDefaultTargetTriple` 三平台可用）；自举侧 `codegen.ax` 在 `gen_module` 时手握 triple 字符串（`codegen.ax:1960`），直接解析。同一台机器上两个编译器对 `target_os()` 必须产出相同值。
- **完整流程**（AGENTS.md 纪律）：spec.md 新增条目 + parser/typecheck builtin 表 + codegen 发射 + `tests/pipeline.rs` 新增用例（`print(target_os())` 断言为本平台值）+ CHANGELOG。

> 备选方案（不推荐）：codegen.ax 内部解析 triple 但不暴露 builtin（能解 S2 解不了 S1——driver.ax 拿不到 triple）；或 `getenv` builtin + `OS` 环境变量启发式（Windows 外不可靠、非确定性）。

### T4.2 `_setmode` 平台化（S2）
`selfhost/codegen.ax:1904-1910`：entry wrapper 中包一层 `if target_os() == "windows"`。Rust 侧 `codegen.rs:469-477` 同步改为基于 triple 的判断（与 T4.1 同源），替换裸 `cfg!(windows)`，保证两侧发射逻辑逐句对应。

### T4.3 `driver.ax` 链接命令（S1）
`selfhost/driver.ax:50`：`/STACK` 仅在 `target_os() == "windows"` 时追加（非 Windows 大概率无标志，见 T0.1 说明）。

### T4.4 固定点测试适配
`selfhost_driver_self_compiles` 的 IR 对比与 object 字节对比逻辑不变；变化的是 object 从 COFF 变为 ELF/Mach-O——对比仍应逐字节成立（两侧同平台同 reloc）。`driver_self_demo.ax:10` 的 `"C:/Program Files/LLVM/lib"` 参数化：由 `target_os()` 分支给出默认 libdir（linux: `/usr/lib/llvm-18`，macos: `/opt/homebrew/opt/llvm/lib`，可被环境覆盖——如新增 `AOXN_LLVM_LIBDIR` 读取，顺带补 `getenv` stdlib binding 或在 demo 内用 `system` 探测）。各 `driver_*_demo.ax` 的 `.exe` 输出名按 `target_os()` 决定是否带扩展名。

### T4.5 自举测试上 CI
M4 完成后，Linux/macOS CI job 增跑 selfhost 系列测试（`cargo test selfhost`），固定点首次在非 Windows 平台成立。

**验收**：ubuntu CI 上 `selfhost_driver_self_compiles` 全绿（IR + ELF object 逐字节一致）；stage-2 产物编译 `examples/` 11/11。

## 7. M5 —— 文档与发布

1. `docs/spec.md` 写入平台声明：Tier 1 = Windows x86_64；Tier 2 = Linux x86_64、macOS x86_64/arm64（列出各自工具链要求：MSVC BuildTools / apt llvm / brew llvm）。
2. `docs/selfhost.md` 增补固定点在新平台的形态；`examples/ffi_llvm.ax` 头注释去掉 Windows-only 语气；AGENTS.md 的 LLVM 路径说明更新。
3. README（如有平台段落）+ CHANGELOG v0.26.0 + 版本号。
4. CI 矩阵固化为 `windows-latest + ubuntu-latest + macos-13 + macos-14`。

## 8. 测试矩阵（迁移完成的定义）

| 验证内容 | Windows | Ubuntu | macOS Intel | macOS arm64 |
|---|---|---|---|---|
| cargo build | ✅ 现状 | M1 | M2 | M3 |
| 93 项集成测试 | ✅ 现状 | M1 | M2 | M3 |
| smoke（primes/strings） | ✅ 现状 | M1 | M2 | M3 |
| examples 11/11（Rust 编译器） | ✅ 现状 | M1 | M2 | M3 |
| 自举固定点（IR + object 字节一致） | ✅ 现状 | M4 | M4 | M4 |
| 自举编译 examples 11/11 | ✅ 现状 | M4 | M4 | M4 |

## 9. 风险登记册

| 风险 | 概率 | 影响 | 缓解 |
|---|---|---|---|
| PIE 重定位错误（B4） | 中 | M1 阻塞 | T1.4 预案：按 triple 切 PIC，一行常量改动；固定点两侧同步 |
| Linux 上测试用例大栈数组溢出 | 低 | 个别测试失败 | RLIMIT_STACK 默认 8MB 与测试假设一致；若溢出，测试内 `ulimit -s unlimited` 或调小用例数组 |
| brew llvm 版本漂移（keg-only 路径/版本号变化） | 中 | M2 CI 间歇失败 | build.rs 探测链覆盖 18–23；CI 固定 `brew install llvm@18` |
| AAPCS64 结构体 sret ABI 与 COFF 差异 | 低-中 | M3 用例失败 | LLVM 层已抽象；失败按用例定位（现有 93 测试即矩阵） |
| 自举/Rust 两侧 `target_os()` 判据不同源导致固定点破裂 | 中 | M4 阻塞 | 硬性纪律：两侧都解析同一 `LLVMGetDefaultTargetTriple()` 字符串；T4.5 首跑即暴露 |
| apt/Homebrew 的 libLLVM 无独立 C API 库符号缺失 | 极低 | 链接失败 | C API 符号自 LLVM 3.x 起全在 libLLVM 中；链接失败时用 `nm -D` 核对 |
| WSL2 与真 Linux CI 环境差异 | 低 | 本地过 CI 挂 | 以 CI 为准；本地仅用于快速迭代 |

## 10. 文件改动清单（速查）

| 文件 | 位置 | 改动 | 里程碑 |
|---|---|---|---|
| `src/platform.rs` | 新增 | 平台判定/扩展名/栈标志集中 | M0 |
| `build.rs` | 全文 | 按 target OS 探测库与链接名；rpath；`AOXN_LLVM_DIR` 统一 | M0 |
| `src/lib.rs` | 293-302, 308, 328, 350, 379 | clang 候选补 brew 路径；obj/ext/stack flag 走 platform | M0–M2 |
| `src/main.rs` | 185, 196 | 扩展名走 platform | M0 |
| `src/codegen.rs` | 17-22, 469-477, 543 | AArch64 注册；`_setmode`/reloc 按 triple | M3/M4 |
| `src/llvm.rs` | 149 附近 | AArch64 四个 extern 声明；`RELOC_PIC` 常量 | M3/T1.4 |
| `tests/pipeline.rs` | ~40 处 | `exe_name()` helper 替换硬编码 | M1 |
| `.github/workflows/ci.yml` | jobs | 增 linux/macos-13/macos-14 job | M1–M3 |
| `selfhost/codegen.ax` | 1904-1910, 1960 | `_setmode` 按 `target_os()`；triple 解析 | M4 |
| `selfhost/driver.ax` | 50 | 栈标志按 `target_os()` | M4 |
| `selfhost/driver_self_demo.ax` 等 | 10 及各 demo | libdir/.exe 参数化 | M4 |
| `docs/spec.md`、`docs/selfhost.md`、`CHANGELOG.md`、`AGENTS.md` | — | 平台声明 + builtin 文档 + 版本 | M4/M5 |
