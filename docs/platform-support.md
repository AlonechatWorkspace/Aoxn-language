# Aoxn 平台支持调查报告（macOS / Linux）

- 日期：2026-09-27
- 针对版本：v0.25.0（commit 8d8a708）
- 调查方式：全量检索 `src/`、`build.rs`、`selfhost/`、`tests/`、`.github/`、`docs/` 中的平台相关代码（`windows` / `msvc` / `setmode` / `COFF` / `triple` / `.exe` / 链接标志 / 环境变量），并逐处核对上下文。

## 一、结论（TL;DR）

**当前不支持 macOS 和 Linux。项目实际支持的平台只有 Windows x86_64**（MSVC 工具链 + Windows 版 LLVM 安装包，CI 也仅在 `windows-latest` 上验证过）。`docs/spec.md` 与 `docs/selfhost.md` 中没有任何平台支持声明，"支持范围"目前是一个无正式承诺的空白。

但这**不是架构级的 Windows 锁定**，而是收尾工程：

- 目标三元组通过 `LLVMGetDefaultTargetTriple()` 动态获取（宿主自适应，非硬编码 `windows-msvc`）；
- `_setmode`（Windows 专属 CRT）在 Rust 编译器中已用 `cfg!(windows)` 守卫；
- clang 查找、PATH 分隔符、输出扩展名均已有跨平台分支；
- 编译器本体零 Windows API 依赖（无 winapi、无宽字符、无 `os::windows`）。

分平台移植难度：**Linux x86_64 ≈ Intel Mac << Apple Silicon**。Linux x86_64 只需 3 处小改动 + CI 验证；Apple Silicon 额外需要注册 AArch64 后端（已验证符号存在于 LLVM-C 库中）。唯一需要语言设计决策的阻塞点在自举编译器一侧（Aoxn 没有条件编译设施）。

## 二、已经可移植的部分（无需改动）

| 位置 | 现状 |
|---|---|
| `src/codegen.rs:519` | `LLVMGetDefaultTargetTriple()` 动态取宿主三元组，模块 target 随之设置 |
| `src/codegen.rs:469-477` | `_setmode` 声明与调用仅在 `cfg!(windows)` 下发射，非 Windows 不产生该符号依赖 |
| `src/lib.rs:293,308` | clang 可执行名（`clang.exe`/`clang`）与 PATH 分隔符（`;`/`:`）按平台分支 |
| `src/main.rs:185` | 输出扩展名 `.exe` 仅 Windows 使用，其他平台无扩展名 |
| `src/llvm.rs` | 全部 FFI 基于 LLVM C API，符号在 Windows 的 `LLVM-C.lib` 与 Linux/macOS 的 `libLLVM.so`/`libLLVM.dylib` 中一致导出 |
| `src/llvm.rs:153` | `CODEGEN_OBJECT_FILE = 1`（目标文件）对 COFF/ELF/Mach-O 通用，`LLVMTargetMachineEmitToFile` 按三元组自动选择格式 |
| `src/lib.rs:287`、各 `AOXN_*` 环境变量 | 全部为普通环境变量，跨平台 |
| C 运行时依赖（malloc/strlen/strcmp/snprintf/printf） | libc 标准符号，三平台 clang 默认链接均可用 |

## 三、macOS / Linux 上的阻塞项

### B1. `build.rs` 只认 Windows 库布局 —— 非 Windows 直接构建失败（阻断）

`build.rs:18-22` 的候选路径全部是 Windows 语义：回退 `C:\Program Files\LLVM`，且以 `lib/LLVM-C.lib` 的存在作为判定条件，随后 `rustc-link-lib=dylib=LLVM-C`。在 Linux/macOS 上：

- 找不到 `LLVM-C.lib` → `build.rs:48` 直接 panic；
- 即使绕过，`-lLLVM-C` 需要的 `libLLVM-C.so` 在发行版/ Homebrew 中通常也不存在——Ubuntu `libllvm19` 提供 `libLLVM-19.so.1`，Homebrew 提供 `libLLVM.dylib`。C API 符号全部在这两个库中，链接名需要按平台选择；
- 另需注意运行时查找：Linux 上需为 `aoxn` 可执行文件设置 rpath 或 `LD_LIBRARY_PATH`（macOS 为 `DYLD_LIBRARY_PATH`/install_name），否则编译出的编译器自身无法加载 libLLVM。

### B2. 链接阶段无条件使用 MSVC 链接器选项（阻断）

`src/lib.rs:379` 无条件追加 `-Wl,/STACK:8388608`（`/STACK` 是 MSVC link.exe 选项，用于给大的栈上数组 8 MB 栈空间）。GNU ld 与 macOS ld64 均不认识该选项，链接必然失败。修法是 `cfg!(windows)` 守卫，非 Windows 按需换成 `-Wl,-z,stacksize=8388608`（Linux）/ `-Wl,-stack_size,0x800000`（macOS），或干脆不设。

### B3. 只注册了 X86 后端 —— Apple Silicon 无法生成代码（阻断于 aarch64-apple-darwin）

`src/codegen.rs:17-22` 的 `init_target()` 仅调用 `LLVMInitializeX86{TargetInfo,Target,TargetMC,AsmPrinter}`。在 Apple Silicon（M 系列，当前所有新 Mac）上三元组为 `arm64-apple-darwin`，`LLVMGetTargetFromTriple` 会报 "No available targets are compatible with triple"。需追加 `LLVMInitializeAArch64*` 四件套。已验证本机 `LLVM-C.lib` 中含 `LLVMInitializeAArch64TargetInfo` 符号，Windows 侧无链接障碍；macOS/Linux 链接的 libLLVM 本身就是全后端。x86_64 Linux 与 Intel Mac 不受此项影响。

### B4. 重定位模型未在 Linux PIE 环境实测（风险，非确证）

Rust 侧与自举侧均以 `RELOC_DEFAULT = 0`（`LLVMRelocDefault`）创建 TargetMachine（`src/llvm.rs:149`、`selfhost/codegen.ax:1975`）。主流 Linux 发行版的 clang 默认以 PIE 链接；若 LLVM 对该三元组解析出的默认重定位模型不是 PIC，会报 "relocation R_X86_64_32S can not be used when making a PIE object"。此项**必须在 Linux CI 上实测**；若出现，将 reloc 常量改为 `LLVMRelocPIC = 2` 即可，代码改动一行。

### S1. 自举驱动无条件使用 `/STACK`（阻断自举链路）

`selfhost/driver.ax:50` 的链接命令硬编码 `-Wl,/STACK:8388608`，同 B2。

### S2. 自举代码生成器无条件发射 `_setmode`（阻断 + 需要设计决策）

`selfhost/codegen.ax:1906-1909` 对每个程序无条件声明并调用 `_setmode(1, 32768)`。glibc/macOS libc 没有该符号，产物在非 Windows 上必然 "undefined symbol: _setmode"。与 Rust 侧不同，**Aoxn 语言本身没有条件编译/平台查询设施**，修法只能二选一：

1. 新增最小语言设施（如编译器内置的 `target_os()` 常量查询或 `std.platform` 模块），自举代码按平台分支；
2. 在驱动层把该调用移出被编译程序（由 `driver.ax` 在链接时按平台追加/省略）——治标，但不需要语言特性。

无论选哪条，都会触及自举固定点（v0.24/v0.25 的 IR 与 COFF 逐字节一致验证），需要重跑 `selfhost_driver_self_compiles` 固定点测试。

### S3. 自举 demo 与测试硬编码 Windows 约定（阻断测试，改动机械）

- `tests/pipeline.rs` 全文约 40 处硬编码 `.exe`（如 `t{id}.exe`、`out.exe`、`selfhost-lex-{id}.exe`）。非 Windows 上编译产物没有扩展名（`src/main.rs:185`），测试会因找不到产物而失败。需抽象为按平台取扩展名。
- `selfhost/driver_self_demo.ax:10` 硬编码 `"C:/Program Files/LLVM/lib"`；各 `driver_*_demo.ax` 输出名均为 `.exe`。

### CI 与文档

- `.github/workflows/ci.yml`：`runs-on: windows-latest`，从未在 Linux/macOS 上验证过任何环节；
- `docs/spec.md` / `docs/selfhost.md`：无任何平台支持声明（检索零命中）。

## 四、平台差异技术要点（移植时参考）

1. **LLVM-C 库的获取方式三平台不同**：Windows 官方安装包只带 `LLVM-C.lib` + `LLVM-C.dll`；Linux 需要发行版包（`libllvm19`，链接名 `-lLLVM-19` 或建 symlink）或 [apt.llvm.org]；macOS 用 `brew install llvm`（`-lLLVM`）。`build.rs` 需要按平台维护候选库名与搜索路径，FFI 代码本身零改动。
2. **栈大小保证**：三平台写法不同（见 B2）。目前测试用例里有依赖大栈数组的场景，Linux/macOS 若不设等价选项需确认 `ulimit -s`/默认主线程栈是否够用。
3. **`_setmode` 二进制 stdout**：仅 Windows 需要（\n → \r\n 翻译问题）；POSIX 天然 \n，无对应需求。
4. **路径**：代码内路径拼接均为 `PathBuf::join`，无手工反斜杠；`src/files.rs` 基于 canonical path，跨平台无虞。仅测试 fixture 与自举 demo 的字面路径需要留意。
5. **ELF/Mach-O 对象发射**：走同一 `LLVMTargetMachineEmitToFile(…, 1)` 路径，无需改动；`AOXN_DUMP_IR` 的验证流程同样适用。

## 五、建议的移植路线（按工作量递增）

**阶段 1 —— Linux x86_64（约半天到一天代码改动 + CI）**

1. `build.rs` 平台化：Linux/macOS 分支按 `libLLVM-C.so` → `libLLVM-XX.so` → `libLLVM.so`（mac: `libLLVM.dylib`）顺序探测，链接名随之；处理 rpath。
2. `src/lib.rs:379` 的 `/STACK` 加 `cfg!(windows)` 守卫。
3. `tests/pipeline.rs` 的 `.exe` 改为平台函数。
4. CI 增加 `ubuntu-latest` job（安装 LLVM，运行完整测试），用实测裁决 B4 的 PIE 风险。

**阶段 2 —— macOS Intel**：同阶段 1，另需验证 `brew llvm` 布局与动态库查找；改动量与阶段 1 相当。

**阶段 3 —— Apple Silicon**：`src/codegen.rs` 增加 `LLVMInitializeAArch64*` 注册；AAPCS64 调用约定由 LLVM 处理，但结构体 sret/传参 ABI 需要跑一遍完整测试矩阵确认。

**阶段 4 —— 自举侧对齐**：先落地 S2 的平台查询决策（建议做成编译器内置，同时是 `main.rs` CLI 语义移植的前置能力）；随后修 `driver.ax`、`codegen.ax`、各 demo，并重跑固定点验证（stage-2 产物与 Rust 编译器逐字节对比在非 Windows 上应改为 ELF/Mach-O 对比）。

**完成后建议在 `docs/spec.md` 写入正式平台声明**，例如："Tier 1: Windows x86_64；Tier 2: Linux x86_64、macOS x86_64/arm64"，并让 CI 矩阵与 Tier 对应。

> 后续实施：见 [platform-migration-plan.md](platform-migration-plan.md)（按里程碑 M0–M5 的详细迁移计划）。

## 六、附：证据清单

| 编号 | 位置 | 内容 |
|---|---|---|
| B1 | `build.rs:10-22,48` | 候选路径仅 Windows 布局；`LLVM-C.lib` 缺失即 panic |
| B2 | `src/lib.rs:379` | 无条件 `-Wl,/STACK:8388608` |
| B3 | `src/codegen.rs:17-22` | 仅注册 X86 后端 |
| B4 | `src/llvm.rs:149`、`selfhost/codegen.ax:1975` | `RELOC_DEFAULT = 0`，未在 PIE 环境验证 |
| S1 | `selfhost/driver.ax:50` | 自举链接命令硬编码 `/STACK` |
| S2 | `selfhost/codegen.ax:1906-1909` | 无条件 `_setmode(1, 32768)` |
| S3 | `tests/pipeline.rs`（多处）、`selfhost/driver_self_demo.ax:10` | 硬编码 `.exe` 与 `C:/Program Files/LLVM/lib` |
| C1 | `tests/pipeline.rs`（多处） | 硬编码库名 `LLVM-C` 与 PATH 分隔符 `;` |
| CI | `.github/workflows/ci.yml` | 四平台矩阵 |
| ✓ | `src/codegen.rs:519` | 三元组动态获取 |
| ✓ | `src/codegen.rs:469-477` | Rust 侧 `_setmode` 已 cfg 守卫 |
| ✓ | `src/lib.rs:293,308`、`src/main.rs:185` | clang 名/PATH 分隔符/扩展名已分支 |
| ✓ | 本机实测 | `LLVM-C.lib` 含 `LLVMInitializeAArch64TargetInfo` 等符号 |

## 七、Tier-2 CI 后续修补（v0.26.2：矩阵首跑暴露）

v0.26.1 的四平台矩阵第一次真跑后，`linux` 与 `macos-arm64` 各失败 5 例。
根因**全在测试与自举侧**（Rust 编译器本体已平台化），本次一并修复：

| 编号 | 失败表现 | 根因 | 修复 |
|---|---|---|---|
| T1 | linux：4 个 selfhost 用例 `cannot find -lLLVM-C` | 测试硬编码 Windows 库名；Ubuntu 的 C API 在版本化的 `libLLVM-18.so` 里 | `platform::llvm_link_name()` 按目录探测（`LLVM-C` → 最新 `libLLVM-<N>` → `libLLVM`），测试统一改用；`AOXN_LLVM_LIB` 可覆盖 |
| T2 | macos-arm64：`no available targets are compatible with triple arm64-apple-darwin` | `selfhost/codegen.ax` 只注册了 X86（B3 的 Rust 侧已修，自举侧漏了） | 补 `LLVMInitializeAArch64{TargetInfo,Target,TargetMC,AsmPrinter}` |
| T3 | linux：自举产物无法链进 PIE 可执行文件 | `selfhost/codegen.ax` 用 `RELOC_DEFAULT`（B4 的自举侧） | 新增 `CG_RELOC()`：Windows 0、其余 2（PIC），与 Rust 侧 `platform::is_windows()` 对齐 |
| T4 | POSIX：自举 demo 找不到 clang；链出的 exe 加载不了 libLLVM | 测试硬编码 PATH 分隔符 `;`（把真 PATH 变成一个不存在的目录）；`-L` 目录未进 loader 搜索路径（Homebrew 的 `libLLVM-C.dylib` 再导出 `@rpath/libLLVM.dylib`） | 测试改用 `std::env::join_paths`；`link_opts` 在 POSIX 上为每个 `-L` 追加 `-Wl,-rpath,<dir>`（`build.rs` 早已为编译器自身这样做） |
| T5 | linux/macOS：`stdlib_system_spawn` 期望 `7` 却得到等待状态 | `system()` 直接暴露 C `system()`：POSIX 返回 wait status（`exit 7` → 1792），Windows 返回退出码 | stdlib 新增 `system_exit_code(cmd)` 归一化（被信号杀死按 shell 惯例报 128+n），测试改用它并统一断言 |

验收：Windows 本地 97/97 全绿（含自举固定点）；Linux x86_64 与 macOS
x86_64/arm64 由 CI 矩阵复验。
