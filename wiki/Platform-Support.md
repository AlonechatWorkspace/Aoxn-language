# 平台支持 · Platform Support

> **中文**：Aoxn 的 Tier 1 平台是 Windows x86_64（全支持），Linux x86_64 与 macOS Apple Silicon 属于由 CI
> 三平台矩阵复验的 Tier 2（Intel Mac 自 v0.27.1 起不再支持）；本页给出各平台工具链（v0.29.0 起只需 clang，无 LLVM）、`src/platform.rs` 平台抽象、语言内建
> `target_os()`，以及至今仍然只在 Windows 上验证的边界。
> **English**: Aoxn's Tier 1 platform is Windows x86_64 (fully supported), while Linux x86_64 and
> Apple Silicon macOS are Tier 2, re-verified by the three-platform CI matrix (Intel Macs are unsupported
> since v0.27.1); this page documents the per-platform
> toolchains (clang only since v0.29.0 — no LLVM), the `src/platform.rs` abstraction, the `target_os()`
> builtin, and the boundaries that are still verified on Windows only.

## 中文

### 支持层级（v0.26.3 现状）

| Tier | 平台 | 状态 | 工具链与 LLVM 获取方式 |
|---|---|---|---|
| **1** | Windows x86_64 | 全支持（本机验证 + CI） | MSVC Build Tools + clang（CI 用 winget 的 LLVM 包提供 clang） |
| **2** | Linux x86_64 | 支持，由 CI 矩阵复验 | `apt-get install -y clang` |
| **2** | macOS arm64（Apple Silicon） | 支持，由 CI 矩阵复验 | 预装 Apple clang（Xcode CLT） |

“由 CI 矩阵复验”的含义是：`.github/workflows/ci.yml` 在这两个 runner 上跑的是**同一套**
`cargo test`（132 个测试：编译 `.ax` → 生成可执行文件 → 运行 → 断言 stdout 与退出码，含 TS 前端与
UI；`aoxn-pkg` 包管理 crate 另有 29 个单元测试）以及 smoke test，而不是“只做了交叉编译”。CI 的
job 结构、步骤与 smoke 断言见 [测试与 CI](Testing-and-CI.md)。

本地开发机（本文档引用的本地测量与自举固定点验证）是 Windows + clang **23.1.0**（winget 的 LLVM
包，只用它的 clang）。自 v0.29.0 起编译生成 C 与最终链接都由 clang 完成，不再探测或链接任何 LLVM
库；`AOXN_CLANG` 可指定 clang 可执行文件路径。

各平台获取 LLVM 的实际命令：

```powershell
# Windows：CI 用的就是这一条（含 --silent），最后从 C:\Program Files\LLVM\bin 找 clang / LLVM-C.dll
winget install --id LLVM.LLVM --accept-source-agreements --accept-package-agreements --silent
```

```bash
# Linux：Tier 2
sudo apt-get update
sudo apt-get install -y llvm-18-dev clang-18 libclang-rt-18-dev
# 让编译器最后的 clang 查找能找到版本化的 clang
sudo ln -sf /usr/lib/llvm-18/bin/clang /usr/local/bin/clang
```

```bash
# macOS：Tier 2（brew 的 llvm 是 keg-only，不在 PATH 上，所以必须设 AOXN_LLVM_DIR）
brew install llvm@18
```

两个容易踩的点：Windows 安装包**只**提供 C API（`LLVM-C.lib` + `LLVM-C.dll`）和少数几个导入库，没有逐组件的
静态 LLVM 库，所以本项目坚持手写 FFI（`src/llvm.rs`），**不要**引入 `inkwell` / `llvm-sys`；Homebrew 的 llvm 是
keg-only，`bin/` 不在 PATH 上，所以两个 macOS job 只做 `brew install llvm@18` 并显式设置 `AOXN_LLVM_DIR`
（keg-only，路径必须自己给出，没有软链）；只有 Linux job 才有
`sudo ln -sf /usr/lib/llvm-18/bin/clang /usr/local/bin/clang || true`，让版本化的 clang 能被编译器的 clang
查找找到。

### LLVM 定位顺序（`build.rs` 的实际实现）

`build.rs`（v0.26.1 起按 `CARGO_CFG_TARGET_OS` 分支）按下面的顺序找**安装根目录**，第一个“库目录里存在可链接
库”的候选胜出：

1. `AOXN_LLVM_DIR`，然后是遗留别名 `AXON_LLVM_DIR`（两者都注册了
   `cargo:rerun-if-env-changed`，改环境变量会触发构建脚本重跑）；
2. `<repo>/LLVM`（仓库内工具链布局，历史上 18.1.8 就放在这里）；
3. `<repo 的父目录>/LLVM`；
4. 平台默认安装位置：
   - Windows：`C:\Program Files\LLVM`；
   - macOS：`/opt/homebrew/opt/llvm`（arm64）、`/usr/local/opt/llvm`（Intel）；
   - Linux：`/usr/lib/llvm-23` … `/usr/lib/llvm-15`（倒序探测）、`/usr/lib/llvm`、`/usr`、`/usr/local`。

每个候选下再按 `<root>/lib`、`<root>/lib64`、`<root>` 依次探测，判据与链接名按平台不同：

| 平台 | 判据（目录中存在即选中） | 链接名 |
|---|---|---|
| Windows | `LLVM-C.lib` | `LLVM-C` |
| Linux | `libLLVM-C.so` → 编号最大的 `libLLVM-<N>.so`（N = 15…23）→ `libLLVM.so` | `LLVM-C` / `LLVM-<N>` / `LLVM` |
| macOS | `libLLVM.dylib` | `LLVM` |

选定后 `build.rs` 打印 `cargo:rustc-link-search=native=<libdir>`、`cargo:rustc-link-lib=dylib=<name>`，另外打印
一条 `cargo:rustc-link-arg=-Wl,-rpath,<libdir>`（注释说明其用途是让编译出的 `aoxn` 在非 Windows 上不必设置
`LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH` 就能加载 libLLVM），最后用
`cargo:warning=aoxn: linking LLVM as '<name>' from <dir>` 把结果打进构建日志。Windows 上还会把
`LLVM-C.dll` 复制到 `target/{debug,release}{,/deps,/examples}`，于是 `cargo run` / `cargo test` 不需要额外设置
PATH 就能加载它。找不到任何候选时 `build.rs` 直接 panic，panic 文案会列出已搜索的路径并提示设置
`AOXN_LLVM_DIR`。

**换了 LLVM 安装后必须让构建脚本重跑**：候选路径与链接结果被 Cargo 缓存在构建脚本输出里，不重跑就仍然链到旧的
LIBPATH。改一下 `build.rs` 的时间戳，或者 `cargo clean`：

```powershell
(Get-Item build.rs).LastWriteTime = Get-Date   # 或 cargo clean
```

```bash
touch build.rs   # 或 cargo clean
```

### 链接名探测：`platform::llvm_link_name()`

编译**用户程序**（尤其是自举的 LLVM 驱动 demo）时，需要把 LLVM C API 库名交给 clang，而这个库名不能硬编码：

- Windows 有专门的 `LLVM-C.lib`，Homebrew 的 macOS 包有专门的 `libLLVM-C.dylib`；
- Debian/Ubuntu 把 C API 符号放在版本化的 `libLLVM-18.so` 里，`-lLLVM-C` 会直接报
  `cannot find -lLLVM-C` —— 这正是 Tier 2 首跑的 T1 失败（见下节）。

`aoxn::platform::llvm_link_name()` 的探测顺序：

1. `AOXN_LLVM_LIB` 环境变量，非空则原样返回（逃生舱，覆盖任何探测结果）；
2. `AOXN_LLVM_DIR` / `AXON_LLVM_DIR`：根目录与它的 `lib/` 两个位置都试；
3. `llvm_dir_candidates()`：Windows 为 `C:\Program Files\LLVM\lib`、`C:\Program Files\LLVM\lib\x64`、
   `C:\LLVM\lib`；macOS 为 `/opt/homebrew/opt/llvm/lib`、`/usr/local/opt/llvm/lib`、
   `/opt/local/libexec/llvm/lib`；Linux 为 `/usr/lib/llvm-23/lib` … `/usr/lib/llvm-15/lib`、`/usr/lib/llvm/lib`、
   `/usr/lib64`、`/usr/lib`；
4. 非 Windows 再补发行版 multiarch 目录：`/usr/lib/x86_64-linux-gnu`、`/usr/lib/aarch64-linux-gnu`、
   `/usr/lib64`、`/usr/lib`（Debian/Ubuntu 的 `libllvm<N>` 运行包把 `libLLVM-<N>.so.1` 放在那里，而那是默认
   链接搜索路径，所以即使 LLVM 安装目录里没有可链接的符号链接也能解析）。

目录内的判定规则由 `llvm_link_name_in(dir)` 提供（公开、可在任意宿主上测试，见
`llvm_link_name_probe_covers_platform_layouts`）：`LLVM-C.*` 或 `libLLVM-C.*`（排除 `LLVM-C.dll`）优先，返回
`LLVM-C`；否则取编号最大的 `libLLVM-<N>.*`，返回 `LLVM-<N>`；否则存在 `libLLVM.*` / `LLVM.lib` 时返回
`LLVM`；都没有则返回 `None`，`llvm_link_name()` 最终回退到 `LLVM-C`。

POSIX 上 `link_opts` 会为每个 `-L <dir>` 追加一个 `-Wl,-rpath,<dir>`：Homebrew 的 `libLLVM-C.dylib` 会再导出
`@rpath/libLLVM.dylib`，只给 `-L` 不给 rpath 时会出现“链接成功、运行时加载失败”（T4）。Windows 的链接器不接受
`-rpath`，因此这个追加带平台守卫。`build.rs` 对编译器自身早已做了同样的事。

### 平台抽象层（`src/platform.rs`）

v0.26.1 把散落在各处的 `cfg!(windows)` 收拢到 `src/platform.rs`，Windows 上的行为与 v0.26.0 逐字节一致：

| 函数 | 语义 | 主要使用点 |
|---|---|---|
| `exe_ext()` | `"exe"`（Windows）/ `""`（其它平台） | `main.rs` 的默认输出名与临时输出名 |
| `obj_ext()` | `"obj"`（Windows，MSVC 惯例）/ `"o"`（ELF / Mach-O 惯例） | `lib.rs` 的目标文件名 |
| `stack_link_flag()` | Windows 返回 `Some("-Wl,/STACK:8388608")`，其它平台返回 `None` | `lib.rs` 的链接参数 |
| `is_windows()` / `is_linux()` / `is_macos()` | `cfg!` 判定的平台谓词 | 重定位模型选择、rpath 追加、`_setmode` 守卫 |
| `llvm_dir_candidates()` | 各平台 LLVM 库目录候选（探测链接名时使用） | `llvm_link_name()`；`build.rs` 有一份等价候选表，因为构建脚本不链接本 crate |
| `target_os_name()` | `"windows"` / `"linux"` / `"macos"` / `"other"` | `codegen.rs` 折叠内建 `target_os()` |
| `llvm_link_name()` / `llvm_link_name_in(dir)` | 上面的链接名探测规则 | `tests/pipeline.rs` 的自举测试、自举驱动 demo |

要点：

- **输出扩展名**：Windows 用 `.exe` / `.obj`，其它平台没有可执行文件扩展名、目标文件用 `.o`。
- **栈链接标志只对 Windows 有意义**：Windows 主线程默认栈 1MB，必须用 MSVC 的 `/STACK:8388608` 提到 8MB（用户
  程序里有大栈数组）；Linux 主线程栈由 `RLIMIT_STACK` 决定（主流发行版默认 8MB），链接器选项对主线程无效；
  macOS 主线程栈固定 8MB。所以 `stack_link_flag()` 在非 Windows 上返回 `None`。
- **目标三元组动态获取**：`codegen.rs` 用 `LLVMGetDefaultTargetTriple()` 取宿主三元组，不硬编码
  `windows-msvc`，因此换平台不需要改源码。
- **X86 与 AArch64 同时注册**：`init_target()` 同时调用 `LLVMInitializeX86{TargetInfo,Target,TargetMC,AsmPrinter}`
  与 `LLVMInitializeAArch64{...}` 四件套（`Once` 语义不变）。两套后端同时注册不冲突，
  `LLVMGetTargetFromTriple` 按三元组选择，Apple Silicon 因此可用。
- **重定位模型**：Linux/macOS 创建 TargetMachine 时用 `RELOC_PIC`（2，因为那里默认链接 PIE），Windows 用
  `RELOC_DEFAULT`（0）。自举侧对应 `CG_RELOC()`：Windows 0，其余 2，与 `platform::is_windows()` 对齐。

### 语言内建 `target_os()`

`docs/spec.md` 的 “Platform query” 一节把它定义为无参函数，返回 `"windows" | "linux" | "macos" | "other"`，
由编译器**在编译期折叠**为模块内私有字符串常量 —— 它不是运行时系统调用。Rust 侧在 `codegen.rs` 里通过
`platform::target_os_name()` 折叠；自举侧 `selfhost/codegen.ax` 的 builtin 分支发射同一个宿主值。

**两侧必须折叠出同一个值**，否则自举固定点立刻破裂：`selfhost_driver_self_compiles` 会比较 Rust 编译器与
Aoxn 编译器对同一程序产出的 IR 文本**和**目标文件字节，任何一处平台分支不同源都会被抓住。目前用到它的地方：

- `selfhost/codegen.ax`：只在 `target_os() == "windows"` 时发射 `_setmode(1, 32768)`（POSIX 的 stdout 本来就是
  二进制安全的）；
- `selfhost/driver.ax`：只在 `target_os() == "windows"` 时给 clang 追加 `-Wl,/STACK:8388608`，并提供
  `exe_suffix()`（Windows `.exe`，其它平台空）；
- `selfhost/driver_self_demo.ax`：按 `target_os()` 选 LLVM 库目录（Windows `C:/Program Files/LLVM/lib`、
  macOS `/opt/homebrew/opt/llvm/lib`、其它 `/usr/lib/llvm-18/lib`）。

### 历史：Tier 2 矩阵首跑暴露的 5 类失败

v0.26.1 的四平台矩阵第一次真跑后，`linux` 与 `macos-arm64` 各失败 5 例；根因**全在测试与自举侧**（Rust 编译器
本体当时已经平台化），v0.26.2 一并修复：

| 编号 | 失败表现 | 根因 | 修复 |
|---|---|---|---|
| T1 | linux：4 个 selfhost 用例 `cannot find -lLLVM-C` | 测试硬编码 Windows 库名；Ubuntu 的 C API 在版本化的 `libLLVM-18.so` 里 | `platform::llvm_link_name()` 按目录探测（`LLVM-C` → 最新 `libLLVM-<N>` → `libLLVM`），`AOXN_LLVM_LIB` 可覆盖 |
| T2 | macos-arm64：`no available targets are compatible with triple arm64-apple-darwin` | 自举 `selfhost/codegen.ax` 只注册了 X86（Rust 侧的同类问题已修，自举侧漏了） | 补 `LLVMInitializeAArch64{TargetInfo,Target,TargetMC,AsmPrinter}` |
| T3 | linux：自举产物无法链进 PIE 可执行文件 | 自举 `codegen.ax` 用 `RELOC_DEFAULT` | 新增 `CG_RELOC()`：Windows 0、其余 2（PIC），与 `platform::is_windows()` 对齐 |
| T4 | POSIX：自举 demo 找不到 clang；链出的 exe 加载不了 libLLVM | 测试硬编码 PATH 分隔符 `;`（把真 PATH 变成一个不存在的目录）；`-L` 目录没有进 loader 搜索路径 | 测试改用 `std::env::join_paths`；`link_opts` 在 POSIX 上为每个 `-L` 追加 `-Wl,-rpath,<dir>` |
| T5 | linux/macOS：`stdlib_system_spawn` 期望 `7` 却拿到等待状态 | `system()` 直接暴露 C `system()`：POSIX 返回 wait status（`exit 7` → 1792），Windows 返回退出码 | stdlib 新增 `system_exit_code(cmd)` 归一化（被信号杀死按 shell 惯例报 128+n），测试改用它 |

验收：Windows 本地 161/161 全绿（pipeline 96 + TS 词法 14 + TS 解析 20 + UI 2 + aoxn-pkg 29，含自举固定点）；Linux x86_64 与 macOS arm64 由 CI 矩阵复验。

### 遗留边界（明确不在当前范围）

- **（已解除）逐字节自举固定点曾只在 Windows 上验证**：v0.29.0 移除 LLVM 依赖后，`selfhost_driver_self_compiles`
  不再要求 `C:\Program Files\LLVM\lib\LLVM-C.lib`，在所有平台真实执行（找不到 clang 时才跳过）。目标文件的
  逐字节比较会屏蔽 COFF 时间戳（clang 每次运行写入的墙钟时间，偏移 4–8 字节）。
- **自举 loader 的窄字符路径限制**：`selfhost/load.ax` 走 stdlib 的 `read_file`，即窄字符 `fopen`，非 ASCII 路径
  在那里会失败；Rust 编译器不受影响。测试 fixture 目录与文件名保持 ASCII。
- **跨编译 / MinGW / 32 位不在范围内**：`--target` 指向非宿主平台、MinGW 工具链、32 位目标都不支持；官方安装包
  分发同样不在范围内（见 `docs/platform-migration-plan.md` 的“目标与非目标”）。

### 迁移计划摘要（M0–M5）

[platform-migration-plan.md](../docs/platform-migration-plan.md) 是这一轮平台化的分期计划，基线 v0.25.0，按里程碑
推进、全部合并后发布 v0.26.0：

| 里程碑 | 内容（摘要） |
|---|---|
| M0 | Rust 侧平台抽象（新增 `src/platform.rs`）+ `build.rs` 平台化（库探测、链接名、rpath、`AOXN_LLVM_DIR` 统一） |
| M1 | Linux x86_64 打通 + CI job（含用实测裁决 PIE 重定位风险） |
| M2 | macOS Intel 打通 + CI job（brew 布局与 keg-only 路径） |
| M3 | Apple Silicon：注册 AArch64 后端 + `macos-14` job |
| M4 | 自举侧对齐：`target_os()` 语言设施、`_setmode` / `/STACK` 平台化、demo 参数化、固定点适配 |
| M5 | 文档、`docs/spec.md` 平台声明、发布 |

该计划文档还给出了非目标、测试矩阵与风险登记册（PIE 重定位、brew 版本漂移、AAPCS64 结构体 ABI 等）；实现与计划
的偏差以 [CHANGELOG.md](../CHANGELOG.md) 的 0.26.x 条目为准。

### 测试如何使用这些 helper

`tests/pipeline.rs` 不在测试里写字面量平台约定：

- 文件头有一个 `EXE` 常量（`if cfg!(windows) { ".exe" } else { "" }`），等价于 `platform::exe_ext()` 的带点形式；
- selfhost 测试用 `llvm_dir()` 定位安装（`AOXN_LLVM_DIR` / `AXON_LLVM_DIR` → `<repo>/LLVM` →
  `C:/Program Files/LLVM`）、`llvm_link_name()`（转发到 `aoxn::platform::llvm_link_name()`）以及
  `path_with_llvm_bin()`（用 `std::env::join_paths` 拼 PATH，避免 POSIX 上把整条 PATH 压成一个不存在的目录）；
- 探测规则本身由 `llvm_link_name_probe_covers_platform_layouts` 在临时目录里直接测（版本化 `libLLVM-18.so` 胜过
  `libLLVM-17.so`、专用 `libLLVM-C.so` 胜过版本化库、`libLLVM.dylib` 回退、空目录返回 `None`）。

## English

### Support tiers (current state, v0.26.3)

| Tier | Platform | Status | Toolchain (clang only — no LLVM since v0.29.0) |
|---|---|---|---|
| **1** | Windows x86_64 | fully supported (local verification + CI) | MSVC Build Tools 2022 + clang (CI takes it from the winget LLVM package) |
| **2** | Linux x86_64 | supported, re-verified by the CI matrix | `apt-get install -y clang` |
| **2** | macOS arm64 (Apple Silicon) | supported, re-verified by the CI matrix | preinstalled Apple clang (Xcode CLT) |

"Re-verified by the CI matrix" means the two Tier 2 runners execute the *same* `cargo test` suite (132
tests: compile `.ax` → produce an executable → run it → assert stdout and exit code, covering the TS front
end and UI too; the `aoxn-pkg` crate adds 29 unit tests of its own)
plus a smoke test — not merely a
cross-compilation check. The job layout, steps and smoke assertions live in [Testing & CI](Testing-and-CI.md).

The local development machine used for the measurements and the self-hosting fixed point quoted in this wiki runs
Windows with clang **23.1.0** (the winget LLVM package, used for its clang only). Since v0.29.0 compiling the
generated C and the final link are both done by clang; no LLVM library is probed or linked anywhere, and
`AOXN_CLANG` selects the clang executable to use.

The actual per-platform commands:

```powershell
# Windows: this is exactly what CI runs (with --silent); clang / LLVM-C.dll come from C:\Program Files\LLVM\bin
winget install --id LLVM.LLVM --accept-source-agreements --accept-package-agreements --silent
```

```bash
# Linux (Tier 2)
sudo apt-get update
sudo apt-get install -y llvm-18-dev clang-18 libclang-rt-18-dev
# make the versioned clang discoverable for the compiler's clang lookup
sudo ln -sf /usr/lib/llvm-18/bin/clang /usr/local/bin/clang
```

```bash
# macOS (Tier 2): brew's llvm is keg-only and not on PATH, hence AOXN_LLVM_DIR
brew install llvm@18
```

Two traps worth knowing: the Windows installer provides **only** the C API (`LLVM-C.lib` + `LLVM-C.dll`) plus a few
other import libraries, not the per-component static LLVM libraries, which is why this project keeps
hand-written FFI
(`src/llvm.rs`) and does **not** use `inkwell` / `llvm-sys`; and Homebrew's llvm is keg-only, so the two macOS jobs
only run `brew install llvm@18` plus an explicit `AOXN_LLVM_DIR` (the keg-only path has to be given, there is no
symlink), while only the Linux job adds
`sudo ln -sf /usr/lib/llvm-18/bin/clang /usr/local/bin/clang || true` so the versioned clang is discoverable by the
compiler's clang lookup.

### LLVM discovery order (what `build.rs` really does)

`build.rs` (platform-branched on `CARGO_CFG_TARGET_OS` since v0.26.1) searches for an **install
root** in this order
and takes the first candidate whose library directory contains a linkable library:

1. `AOXN_LLVM_DIR`, then the legacy alias `AXON_LLVM_DIR` (both register `cargo:rerun-if-env-changed`, so changing
   the variable re-runs the build script);
2. `<repo>/LLVM` (the in-repo toolchain layout; LLVM 18.1.8 used to live there);
3. `<parent of repo>/LLVM`;
4. platform defaults:
   - Windows: `C:\Program Files\LLVM`;
   - macOS: `/opt/homebrew/opt/llvm` (arm64), `/usr/local/opt/llvm` (Intel);
   - Linux: `/usr/lib/llvm-23` … `/usr/lib/llvm-15` (newest first), `/usr/lib/llvm`, `/usr`, `/usr/local`.

Within each candidate the probe tries `<root>/lib`, `<root>/lib64` and `<root>` itself. The
acceptance criterion and
the resulting link name differ per platform:

| Platform | Criterion (present in the directory) | Link name |
|---|---|---|
| Windows | `LLVM-C.lib` | `LLVM-C` |
| Linux | `libLLVM-C.so` → highest `libLLVM-<N>.so` (N = 15…23) → `libLLVM.so` | `LLVM-C` / `LLVM-<N>` / `LLVM` |
| macOS | `libLLVM.dylib` | `LLVM` |

Once a candidate wins, `build.rs` emits `cargo:rustc-link-search=native=<libdir>` and
`cargo:rustc-link-lib=dylib=<name>`, plus a `cargo:rustc-link-arg=-Wl,-rpath,<libdir>` line (its comment states the
purpose: let the produced `aoxn` binary load libLLVM on non-Windows hosts without `LD_LIBRARY_PATH` /
`DYLD_LIBRARY_PATH`), and finally a `cargo:warning=aoxn: linking LLVM as '<name>' from <dir>` so
the choice shows up
in build logs. On Windows it also copies `LLVM-C.dll` into
`target/{debug,release}{,/deps,/examples}`, so `cargo run`
and `cargo test` load it without any PATH setup. When nothing is found, `build.rs` panics with the
searched candidate
list and a hint to set `AOXN_LLVM_DIR`.

**After changing the LLVM install, force the build script to re-run**: the candidate paths and the link result are
cached in the build-script output, so without a re-run you keep linking against the old LIBPATH.
Touch `build.rs`, or
`cargo clean`:

```powershell
(Get-Item build.rs).LastWriteTime = Get-Date   # or: cargo clean
```

```bash
touch build.rs   # or: cargo clean
```

### Link-name probing: `platform::llvm_link_name()`

Compiling a **user program** (notably the self-hosted LLVM driver demos) requires handing the LLVM C
API library name
to clang, and that name cannot be hardcoded:

- Windows ships a dedicated `LLVM-C.lib`, and Homebrew's macOS package ships `libLLVM-C.dylib`;
- Debian/Ubuntu put the C API symbols inside a versioned `libLLVM-18.so`, so `-lLLVM-C` fails outright with
  `cannot find -lLLVM-C` — exactly the T1 failure in the first Tier 2 run (next section).

`aoxn::platform::llvm_link_name()` probes in this order:

1. the `AOXN_LLVM_LIB` environment variable — if non-empty it is returned verbatim (an escape hatch that overrides
   any probe);
2. `AOXN_LLVM_DIR` / `AXON_LLVM_DIR`: both the install root and its `lib/` are tried;
3. `llvm_dir_candidates()`: on Windows `C:\Program Files\LLVM\lib`, `C:\Program Files\LLVM\lib\x64`,
   `C:\LLVM\lib`; on macOS `/opt/homebrew/opt/llvm/lib`, `/usr/local/opt/llvm/lib`,
   `/opt/local/libexec/llvm/lib`; on Linux `/usr/lib/llvm-23/lib` … `/usr/lib/llvm-15/lib`, `/usr/lib/llvm/lib`,
   `/usr/lib64`, `/usr/lib`;
4. on non-Windows hosts, the distro multiarch directories `/usr/lib/x86_64-linux-gnu`,
   `/usr/lib/aarch64-linux-gnu`,
   `/usr/lib64`, `/usr/lib` are appended (Debian/Ubuntu's `libllvm<N>` runtime package puts `libLLVM-<N>.so.1`
   there, and that is a default linker search path, so `-lLLVM-<N>` resolves even when the LLVM install directory
   holds no linkable file at all).

The in-directory rule is exposed as `llvm_link_name_in(dir)` (public and testable on any host, see
`llvm_link_name_probe_covers_platform_layouts`): a `LLVM-C.*` or `libLLVM-C.*` file (excluding
`LLVM-C.dll`) wins and
yields `LLVM-C`; otherwise the highest-numbered `libLLVM-<N>.*` yields `LLVM-<N>`; otherwise a `libLLVM.*` /
`LLVM.lib` yields `LLVM`; if none is present the probe returns `None` and `llvm_link_name()` finally falls back to
`LLVM-C`.

On POSIX, `link_opts` appends `-Wl,-rpath,<dir>` for every `-L <dir>`: Homebrew's `libLLVM-C.dylib` re-exports
`@rpath/libLLVM.dylib`, so passing `-L` without an rpath links fine but fails at load time (T4). Windows linkers
reject `-rpath`, hence the platform guard. `build.rs` already did the same thing for the compiler itself.

### The platform abstraction layer (`src/platform.rs`)

v0.26.1 collected the scattered `cfg!(windows)` branches into `src/platform.rs`; on Windows the behavior is
byte-identical to v0.26.0:

| Function | Semantics | Main call sites |
|---|---|---|
| `exe_ext()` | `"exe"` on Windows, `""` elsewhere | `main.rs` default and temporary output names |
| `obj_ext()` | `"obj"` on Windows (MSVC convention), `"o"` elsewhere (ELF / Mach-O) | `lib.rs` object file naming |
| `stack_link_flag()` | `Some("-Wl,/STACK:8388608")` on Windows, `None` elsewhere | `lib.rs` link arguments |
| `is_windows()` / `is_linux()` / `is_macos()` | `cfg!`-based predicates | relocation model choice, rpath appending, `_setmode` guard |
| `llvm_dir_candidates()` | per-platform LLVM library directory candidates | `llvm_link_name()`; `build.rs` keeps an equivalent table of its own because build scripts do not link this crate |
| `target_os_name()` | `"windows"` / `"linux"` / `"macos"` / `"other"` | `codegen.rs` folding the `target_os()` builtin |
| `llvm_link_name()` / `llvm_link_name_in(dir)` | the probing rules above | the self-host tests in `tests/pipeline.rs`, the self-hosted driver demos |

The essentials:

- **Output extensions**: `.exe` / `.obj` on Windows; no executable extension elsewhere and `.o` for objects.
- **The stack link flag only matters on Windows**: the Windows main thread has a 1MB stack, so user programs with
  large stack arrays need MSVC's `/STACK:8388608` to raise it to 8MB. On Linux the main-thread stack is governed by
  `RLIMIT_STACK` (8MB by default on mainstream distros) and linker options do not affect the main
  thread; on macOS it
  is fixed at 8MB. Hence `stack_link_flag()` returns `None` off Windows.
- **The target triple is dynamic**: `codegen.rs` uses `LLVMGetDefaultTargetTriple()` instead of hardcoding
  `windows-msvc`, so moving between platforms needs no source change.
- **X86 and AArch64 are both registered**: `init_target()` calls
  `LLVMInitializeX86{TargetInfo,Target,TargetMC,AsmPrinter}`
  and `LLVMInitializeAArch64{...}` (the `Once` semantics are unchanged). Registering both does not conflict —
  `LLVMGetTargetFromTriple` selects by triple — which is what makes Apple Silicon work.
- **Relocation model**: the target machine is created with `RELOC_PIC` (2, because Linux/macOS link
  PIE by default) on
  Linux/macOS and `RELOC_DEFAULT` (0) on Windows. The self-hosted counterpart is `CG_RELOC()`: 0 on Windows, 2
  elsewhere, mirroring `platform::is_windows()`.

### The `target_os()` language builtin

`docs/spec.md` ("Platform query") defines it as a nullary function returning
`"windows" | "linux" | "macos" | "other"`,
**folded at compile time** into a module-internal string constant — it is not a runtime syscall. The
Rust side folds it
in `codegen.rs` through `platform::target_os_name()`; the self-hosted side emits the same host value in the builtin
branch of `selfhost/codegen.ax`.

**Both compilers must fold the same value**, otherwise the self-hosting fixed point breaks immediately:
`selfhost_driver_self_compiles` compares the IR text *and* the object file bytes produced for the
same program by the
Rust compiler and the Aoxn-built compiler, so any platform branch that disagrees between the two implementations is
caught. Current users of the builtin:

- `selfhost/codegen.ax`: emits `_setmode(1, 32768)` only when `target_os() == "windows"` (POSIX stdout is already
  binary-safe);
- `selfhost/driver.ax`: appends `-Wl,/STACK:8388608` to the clang command only when `target_os() == "windows"`, and
  provides `exe_suffix()` (`.exe` on Windows, empty elsewhere);
- `selfhost/driver_self_demo.ax`: picks the LLVM library directory by `target_os()` (Windows
  `C:/Program Files/LLVM/lib`, macOS `/opt/homebrew/opt/llvm/lib`, otherwise `/usr/lib/llvm-18/lib`).

### History: the five failure classes the first Tier 2 run exposed

When the four-platform matrix ran for the first time in v0.26.1, `linux` and `macos-arm64` failed
five cases each. The
root causes were **all on the test / self-hosted side** (the Rust compiler itself was already platformized), and
v0.26.2 fixed them together:

| ID | Symptom | Root cause | Fix |
|---|---|---|---|
| T1 | linux: four selfhost cases failed with `cannot find -lLLVM-C` | tests hardcoded the Windows library name; Ubuntu keeps the C API in a versioned `libLLVM-18.so` | `platform::llvm_link_name()` probes the directory (`LLVM-C` → newest `libLLVM-<N>` → `libLLVM`); `AOXN_LLVM_LIB` overrides |
| T2 | macos-arm64: `no available targets are compatible with triple arm64-apple-darwin` | the self-hosted `selfhost/codegen.ax` registered only X86 (the Rust-side twin of that bug was fixed; the self-hosted side was missed) | register `LLVMInitializeAArch64{TargetInfo,Target,TargetMC,AsmPrinter}` |
| T3 | linux: a self-hosted product could not link into a PIE executable | the self-hosted `codegen.ax` used `RELOC_DEFAULT` | new `CG_RELOC()`: 0 on Windows, 2 (PIC) elsewhere, aligned with `platform::is_windows()` |
| T4 | POSIX: the self-hosted demos could not find clang; the linked exe could not load libLLVM | tests hardcoded the `;` PATH separator (collapsing the real PATH into one nonexistent directory); `-L` directories never reached the loader search path | tests use `std::env::join_paths`; `link_opts` appends `-Wl,-rpath,<dir>` per `-L` on POSIX |
| T5 | linux/macOS: `stdlib_system_spawn` expected `7` but got a wait status | `system()` exposed the raw C `system()`: POSIX returns a wait status (`exit 7` → 1792), Windows returns the exit code | the stdlib gained `system_exit_code(cmd)` to normalize both (a signal-killed process is reported shell-style as 128+n); the test uses it |

Acceptance: 161/161 green locally on Windows (pipeline 96 + TS lex 14 + TS parse 20 + UI 2 + aoxn-pkg 29, including
the self-hosting fixed point); Linux x86_64 and macOS
x86_64/arm64 are re-verified by the CI matrix.

### Boundaries that are explicitly out of scope

- **(Resolved) The byte-exact self-hosting fixed point used to be verified on Windows only**: after v0.29.0 removed
  the LLVM dependency, `selfhost_driver_self_compiles` no longer requires `C:\Program Files\LLVM\lib\LLVM-C.lib` and
  really runs on every platform (it skips only when clang cannot be found). The object comparison masks the COFF
  TimeDateStamp (the wall clock each clang run writes in, bytes 4–8).
- **The self-hosted loader is limited to narrow-character paths**: `selfhost/load.ax` goes through the stdlib
  `read_file`, i.e. narrow `fopen`, so non-ASCII paths fail there; the Rust compiler is unaffected. Test fixture
  directories and file names stay ASCII.
- **Cross-compilation, MinGW and 32-bit targets are out of scope**: no `--target` pointing at a
  non-host platform, no
  MinGW toolchain, no 32-bit targets — and no official installer distribution either (see the
  goals/non-goals section
  of `docs/platform-migration-plan.md`).

### Migration plan summary (M0–M5)

[platform-migration-plan.md](../docs/platform-migration-plan.md) is the phased plan behind this port, baselined on
v0.25.0 and released as v0.26.0 once every milestone landed:

| Milestone | Content (summary) |
|---|---|
| M0 | Rust-side platform abstraction (new `src/platform.rs`) + `build.rs` port (library probe, link name, rpath, `AOXN_LLVM_DIR` unification) |
| M1 | Linux x86_64 working + CI job (including settling the PIE relocation risk with real measurements) |
| M2 | Intel macOS working + CI job (brew layout and keg-only paths) |
| M3 | Apple Silicon: register the AArch64 backend + the `macos-14` job |
| M4 | Self-hosted side alignment: the `target_os()` facility, platform-gated `_setmode` / `/STACK`, demo parameterization, fixed-point adaptation |
| M5 | Docs, the `docs/spec.md` platform declaration, release |

The plan document also carries the non-goals, the verification matrix and a risk register (PIE relocation, brew
version drift, the AAPCS64 struct ABI, …); where implementation deviates from the plan, the 0.26.x entries in
[CHANGELOG.md](../CHANGELOG.md) are authoritative.

### How the tests use these helpers

`tests/pipeline.rs` avoids literal platform conventions:

- the file header defines an `EXE` constant (`if cfg!(windows) { ".exe" } else { "" }`), the dotted equivalent of
  `platform::exe_ext()`;
- the selfhost tests locate the install with `llvm_dir()` (`AOXN_LLVM_DIR` / `AXON_LLVM_DIR` → `<repo>/LLVM` →
  `C:/Program Files/LLVM`), fetch the library name with `llvm_link_name()` (forwarding to
  `aoxn::platform::llvm_link_name()`), and build PATH with `path_with_llvm_bin()` (via `std::env::join_paths`, so
  POSIX does not collapse the whole PATH into one nonexistent directory);
- the probing rules themselves are exercised by `llvm_link_name_probe_covers_platform_layouts` over temporary
  directories (a versioned `libLLVM-18.so` beats `libLLVM-17.so`, a dedicated `libLLVM-C.so` beats
  the versioned one,
  `libLLVM.dylib` falls back to `LLVM`, and an empty directory yields `None`).

---

## 源文件 / Source files

- [build.rs](../build.rs) — `AOXN_LLVM_DIR` / `<repo>/LLVM` / platform defaults, per-OS probe and link name, rpath,
  Windows `LLVM-C.dll` staging, panic text.
- [src/platform.rs](../src/platform.rs) — `exe_ext`, `obj_ext`, `stack_link_flag`, `is_windows/linux/macos`,
  `llvm_dir_candidates`, `target_os_name`, `llvm_link_name`, `llvm_link_name_in`.
- [src/codegen.rs](../src/codegen.rs) — AArch64 + X86 registration, `LLVMGetDefaultTargetTriple`, `RELOC_PIC` vs
  `RELOC_DEFAULT`, `cfg`-gated `_setmode`, `target_os()` folding (`platform::target_os_name()`).
- [src/lib.rs](../src/lib.rs) — `obj_ext()` naming, `stack_link_flag()`, POSIX `-rpath` appending, clang lookup.
- [src/main.rs](../src/main.rs) — `exe_ext()` for default and temporary output names.
- [selfhost/codegen.ax](../selfhost/codegen.ax) — AArch64 registration, `CG_RELOC()`, Windows-only `_setmode`,
  `target_os` builtin emission.
- [selfhost/driver.ax](../selfhost/driver.ax) — `exe_suffix()`, Windows-only `/STACK` link flag.
- [selfhost/driver_self_demo.ax](../selfhost/driver_self_demo.ax) — `target_os()`-selected libdir, hardcoded
  `"LLVM-C"` link name (the Windows-only fixed-point boundary).
- [tests/pipeline.rs](../tests/pipeline.rs) — `EXE`, `llvm_dir()`, `llvm_link_name()`, `path_with_llvm_bin()`,
  `llvm_link_name_probe_covers_platform_layouts`, the selfhost/fixed-point tests.
- [docs/platform-support.md](../docs/platform-support.md) — the original
  survey (B1–B4, S1–S3) and §7's T1–T5 table.
- [docs/platform-migration-plan.md](../docs/platform-migration-plan.md) — M0–M5 milestones, goals/non-goals, risk
  register, test matrix.
- [docs/spec.md](../docs/spec.md) — "Platform query" (`target_os()`) and "Platform support" (Tier table).
- [.github/workflows/ci.yml](../.github/workflows/ci.yml) — the four-platform matrix and its LLVM install steps.
- [CHANGELOG.md](../CHANGELOG.md) — 0.26.0–0.26.3 entries (platform migration, Tier-2 fixes, verification notes).
- [CONTRIBUTING.md](../CONTRIBUTING.md) — Tier 1/Tier 2 prerequisites, the "don't use inkwell/llvm-sys" rule.
- [Cargo.toml](../Cargo.toml) — version baseline v0.26.3.
