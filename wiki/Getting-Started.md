# 快速上手 · Getting Started

> **中文**：从零到跑起第一个 Aoxn 程序：环境准备、构建编译器、编译运行、以及第一次最容易踩的几个坑。
> **English**: From nothing to your first running Aoxn program: prerequisites, building the compiler, compiling and running, and the traps that bite first.

## 中文

### 1. 前置条件

Aoxn 编译器用 Rust 写，最终链接依赖 clang，代码生成依赖 LLVM 的 **C API**。

| 依赖 | 说明 |
|---|---|
| Rust（stable，`x86_64-pc-windows-msvc` 宿主） | `cargo build` / `cargo test` |
| LLVM（带 C API） | 本地验证版本 23.1.0；Tier 2 的 CI 使用 LLVM 18。`build.rs` 按 `AOXN_LLVM_DIR` → `<仓库>/LLVM` → `C:\Program Files\LLVM` 顺序查找 |
| clang | 最后一步链接由它完成；查找顺序：`AOXN_CLANG` → `PATH` → 仓库内 `LLVM\bin\clang.exe` → `C:\Program Files\LLVM\bin\clang.exe` |
| MSVC Build Tools 2022 | clang 自动探测它；MSVC 宿主必需 |

各平台的安装方式（与 CI 矩阵一致）：

```powershell
# Windows（Tier 1）
winget install --id LLVM.LLVM --accept-source-agreements --accept-package-agreements
$env:AOXN_LLVM_DIR = "C:\Program Files\LLVM"
```

```bash
# Linux（Tier 2）
sudo apt-get install -y llvm-18-dev clang-18 libclang-rt-18-dev
export AOXN_LLVM_DIR=/usr/lib/llvm-18
```

```bash
# macOS（Tier 2）— brew 的 llvm 是 keg-only，必须显式指定路径
brew install llvm@18
export AOXN_LLVM_DIR=/opt/homebrew/opt/llvm@18   # Intel Mac: /usr/local/opt/llvm@18
```

> Windows 安装包只带 `LLVM-C.lib` + `LLVM-C.dll`（以及少数几个导入库），**没有**按组件拆分的静态 LLVM 库。因此本项目手写 LLVM-C FFI，不引入 `inkwell` / `llvm-sys`。
> 如果换了 LLVM 安装位置而 `cargo build` 仍然链接旧库，touch 一下 `build.rs`（或 `cargo clean`）让构建脚本重跑。

### 2. 构建编译器

```powershell
git clone https://github.com/Ryan-178/Aoxn-language.git
cd Aoxn-language
cargo build
cargo test          # 97 个端到端测试：编译 → 运行 → 断言输出
```

`cargo test` 每个用例都要真正编译并运行一个可执行文件，所以比普通 Rust crate 慢。改大程序时可以用 `--O1` 让编译快近一半（O3 仍是默认档，也是"与 `clang -O3` 同性能"这一承诺对应的档位）。

### 3. 第一个程序

新建 `hello.ax`：

```aoxn
# the classic
def main() -> int:
    print("hello, Aoxn")
    return 0
```

编译并运行：

```powershell
cargo run -- run hello.ax          # 编译 + 运行，一条命令
cargo run -- build hello.ax -o hello.exe
.\hello.exe                        # hello, Aoxn
```

`aoxn` 有三个子命令，日常够用：

| 命令 | 作用 |
|---|---|
| `aoxn run <file.ax> [-- args...]` | 编译并立即运行；程序参数写在 `--` 之后 |
| `aoxn build <file.ax> [-o out]` | 产出独立可执行文件（默认输出 `<file>.exe`） |
| `aoxn ir <file.ax>` | 打印优化后的 LLVM IR，用来确认"源码到底变成了什么" |

完整旗标、优化级别、构建缓存与环境变量见 [命令行与工具链](CLI-and-Tooling.md)。

### 4. 用标准库与多文件

`stdlib/stdlib.ax` 是**用 Aoxn 自己写**的标准库；`import` 的路径相对于**导入者所在文件**解析，每个文件只被包含一次：

```aoxn
import "../stdlib/stdlib.ax"

def main() -> int:
    nums = [5, 3, 8, 1]
    s = sort(nums)                 # 泛型，按 (T, N) 单态化
    print(f"min={min_of(s)}, max={max_of(s)}, sum={sum_int(s)}")
    return 0
```

```powershell
cargo run -- run myprog.ax
```

Aoxn 没有包管理器也没有模块限定名：所有 `import` 进来的文件合并进**同一个命名空间**，因此重名会冲突，路径也必须自己写清楚。

### 5. 第一次最容易踩的坑

| 现象 | 原因 |
|---|---|
| 1:1 报 "unexpected character" | 文件带了 UTF-8 BOM。Aoxn 按字节读源码，PowerShell 5.1 的 `-Encoding UTF8` 会加 BOM——用编辑器保存为「UTF-8 无 BOM」 |
| `// 注释` 报错或算出奇怪结果 | `//` 不是注释（注释只有 `#`），也不是运算符：`5 // 2` 直接是解析错误。`/` 对两个 `int` 就是整数除法 |
| 行尾写 `;` | 分号不是语句终止符，只在 `[T; N]` 数组类型里出现 |
| `x = 1 + 1.5` 被拒绝 | 没有隐式 `int`/`float` 转换；`1.0 + 1.5` 才对 |
| `if x:` 里 `x` 是 `int` | 条件必须是 `bool`；写 `if x != 0:` |
| 函数少一条 `return` | 非 `void` 函数必须在**所有**路径返回值，末尾没有兜底 return 也报错 |
| `arr[10]` 越界但不报错 | 数组下标**不检查**（C 风格），越界是未定义行为 |
| 缩进混用 Tab 和空格 | 缩进定义块结构；Tab 按 4 列计算，混用会触发缩进不匹配 |

更多报错与定位方法见 [常见问题与排错](Troubleshooting-FAQ.md)。

### 6. 下一步

- 想快速掌握语法 → [语言速览](Language-Tour.md)
- 想知道每条规则 → [语言参考](Language-Reference.md)
- 想知道标准库里有什么 → [标准库](Standard-Library.md)
- 想读到 IR 层面 → [代码生成与 LLVM](Codegen-and-LLVM-FFI.md)

## English

### 1. Prerequisites

The Aoxn compiler is written in Rust, the final link shells out to clang, and
code generation needs LLVM's **C API**.

| Requirement | Notes |
|---|---|
| Rust (stable, `x86_64-pc-windows-msvc` host) | `cargo build` / `cargo test` |
| LLVM with the C API | 23.1.0 verified locally; the Tier 2 CI jobs use LLVM 18. `build.rs` searches `AOXN_LLVM_DIR` → `<repo>/LLVM` → `C:\Program Files\LLVM` |
| clang | Performs the final link; lookup order is `AOXN_CLANG` → `PATH` → repo-local `LLVM\bin\clang.exe` → `C:\Program Files\LLVM\bin\clang.exe` |
| MSVC Build Tools 2022 | clang auto-detects them; required for the MSVC host |

Installation per platform (identical to the CI matrix):

```powershell
# Windows (Tier 1)
winget install --id LLVM.LLVM --accept-source-agreements --accept-package-agreements
$env:AOXN_LLVM_DIR = "C:\Program Files\LLVM"
```

```bash
# Linux (Tier 2)
sudo apt-get install -y llvm-18-dev clang-18 libclang-rt-18-dev
export AOXN_LLVM_DIR=/usr/lib/llvm-18
```

```bash
# macOS (Tier 2) — brew's llvm is keg-only, so point at it explicitly
brew install llvm@18
export AOXN_LLVM_DIR=/opt/homebrew/opt/llvm@18   # Intel Mac: /usr/local/opt/llvm@18
```

> The Windows installer ships only `LLVM-C.lib` + `LLVM-C.dll` (plus a few
> other import libraries) — **not** the per-component static LLVM libraries.
> That is why this project hand-writes LLVM-C FFI instead of using
> `inkwell` / `llvm-sys`.
> If you move your LLVM install and `cargo build` still links the old library,
> touch `build.rs` (or `cargo clean`) so the build script re-runs.

### 2. Build the compiler

```powershell
git clone https://github.com/Ryan-178/Aoxn-language.git
cd Aoxn-language
cargo build
cargo test          # 97 end-to-end tests: compile -> run -> assert output
```

`cargo test` really compiles and runs an executable per case, so a full run is
slower than a typical Rust crate. While iterating on something big, `--O1`
roughly halves compile time (O3 remains the default and the level behind the
"parity with `clang -O3`" promise).

### 3. Your first program

Create `hello.ax`:

```aoxn
# the classic
def main() -> int:
    print("hello, Aoxn")
    return 0
```

Compile and run it:

```powershell
cargo run -- run hello.ax          # compile + run in one step
cargo run -- build hello.ax -o hello.exe
.\hello.exe                        # hello, Aoxn
```

Three subcommands cover everyday use:

| Command | Purpose |
|---|---|
| `aoxn run <file.ax> [-- args...]` | compile and run immediately; program arguments go after `--` |
| `aoxn build <file.ax> [-o out]` | produce a standalone executable (default output `<file>.exe`) |
| `aoxn ir <file.ax>` | print the optimized LLVM IR, to see what the source became |

Flags, optimization levels, the build cache and environment variables are all in
[CLI and Tooling](CLI-and-Tooling.md).

### 4. The standard library and multiple files

`stdlib/stdlib.ax` is the standard library, **written in Aoxn itself**.
`import` paths resolve relative to **the importing file**, and each file is
included exactly once:

```aoxn
import "../stdlib/stdlib.ax"

def main() -> int:
    nums = [5, 3, 8, 1]
    s = sort(nums)                 # generic, monomorphized per (T, N)
    print(f"min={min_of(s)}, max={max_of(s)}, sum={sum_int(s)}")
    return 0
```

```powershell
cargo run -- run myprog.ax
```

There is no package manager and no module qualification: every imported file is
merged into **one namespace**, so duplicate names collide and you spell out the
paths yourself.

### 5. Traps that bite first

| Symptom | Cause |
|---|---|
| "unexpected character" at 1:1 | The file starts with a UTF-8 BOM. Aoxn reads sources byte-wise, and PowerShell 5.1's `-Encoding UTF8` adds one — save as "UTF-8 without BOM" |
| `// comment` errors or computes something odd | `//` is neither a comment (only `#` is) nor an operator: `5 // 2` is a parse error. `/` on two `int`s is integer division |
| A trailing `;` | Semicolons are not statement terminators; `;` only appears inside `[T; N]` |
| `x = 1 + 1.5` is rejected | There are no implicit `int`/`float` conversions; write `1.0 + 1.5` |
| `if x:` with an `int` `x` | Conditions must be `bool`; write `if x != 0:` |
| A function "does not return a value on all paths" | Non-`void` functions must return on **every** path; a missing trailing `return` is an error |
| `arr[10]` is out of bounds but nothing complains | Indexing is **unchecked** (C-style); out-of-bounds is undefined behaviour |
| Mixed tabs and spaces | Indentation defines blocks; a tab counts as 4 columns, and mixing them trips the indent mismatch check |

More on errors and how to locate them: [Troubleshooting and FAQ](Troubleshooting-FAQ.md).

### 6. Next steps

- Want the syntax fast → [Language Tour](Language-Tour.md)
- Want every rule → [Language Reference](Language-Reference.md)
- Want to know what ships in the stdlib → [Standard Library](Standard-Library.md)
- Want to read the IR → [Codegen and LLVM](Codegen-and-LLVM-FFI.md)

---

## 源文件 / Source files

- [README.md](../README.md) — quick start (its Status section is outdated)
- [CONTRIBUTING.md](../CONTRIBUTING.md) — prerequisites, build and test commands, debug switches
- [.github/workflows/ci.yml](../.github/workflows/ci.yml) — the per-platform install steps
- [examples/hello.ax](../examples/hello.ax) — the hello world program
- [examples/stdlib_demo.ax](../examples/stdlib_demo.ax) — a program importing the stdlib
- [build.rs](../build.rs) — LLVM discovery and the clang search path
- [docs/spec.md](../docs/spec.md) — the language specification
