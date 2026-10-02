# Installing Aoxn

**One file.** `Aoxn-<version>-Setup.exe` carries the entire toolchain — the
compiler, the standard library, the UI toolkit and the examples — and
installing it is a double-click. Aoxn targets **Windows** (x86_64).

## Install

Double-click it. A window appears with a progress bar and a log: it unpacks
the toolchain, adds `aoxn` to your PATH, makes sure a C toolchain is present
and finishes by running `aoxn doctor`, which compiles and executes a test
program. The button turns into **Finish** only when the toolchain actually
works.

Where things land:

```text
%LOCALAPPDATA%\aoxn\          (override with -Prefix <dir> or %AOXN_HOME%)
  bin\aoxn.exe                the compiler driver
  lib\stdlib\*.ax             standard library + UI toolkit
  examples\*.ax               runnable samples
  docs\install.md             this file
  LICENSE  README.md  CHANGELOG.md
```

Open a **new** terminal afterwards so it picks up the updated PATH.

Scripted / unattended installs (no window, progress on stdout):

```powershell
.\Aoxn-0.30.0-Setup.exe -Console -Prefix C:\Tools\aoxn
```

| flag | meaning |
| --- | --- |
| `-Console`, `-Quiet` | no window; write progress to stdout (CI, scripts) |
| `-Prefix <dir>` | install root (default: `$AOXN_HOME`, else `%LOCALAPPDATA%\aoxn`) |
| `-NoClang` | do not try to install the C toolchain |
| `-Uninstall` | remove the install and the PATH entry |
| `-?` | help |

Uninstalling is the same file with `-Uninstall`.

## The C toolchain

Aoxn lowers programs to ISO C and hands them to **clang**; the compiler
executable is not self-contained. The installer looks for clang in this order:

1. `$AOXN_CLANG` — an explicit path wins over everything else
2. `PATH`
3. `%LOCALAPPDATA%\aoxn\toolchain\bin\clang.exe` — drop a portable LLVM here
   to make the install self-contained
4. `C:\Program Files\LLVM\bin\clang.exe`

If none matches and you did not pass `-NoClang`, the installer runs
`winget install LLVM.LLVM`.

**Linking** on Windows also needs the **MSVC Build Tools** — clang detects
them automatically, and the `aoxn doctor` smoke test reports them when they
are missing:

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools `
  --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools"
```

## Verify

```powershell
aoxn doctor
```

```console
Aoxn 0.30.0 doctor
  [    ] version      0.30.0
  [    ] platform     windows x86_64 (target_os() = windows)
  [ok  ] install root C:\Users\you\AppData\Local\aoxn
  [ok  ] stdlib       C:\Users\you\AppData\Local\aoxn\lib\stdlib
  [    ] examples     C:\Users\you\AppData\Local\aoxn\examples (14 .ax files)
  [ok  ] clang        C:\Program Files\LLVM\bin\clang.exe  clang version 20.1.8
  [ok  ] smoke test   ok  (aoxn-doctor-ok)
  [    ] cache dir    C:\work\target\cache

status: ok — `aoxn run <file.ax>` is ready to use.
```

`aoxn doctor --json` emits the same report as JSON for scripts and agents;
`--no-smoke` skips the compile step.

## Use the standard library

With an install in place, `stdlib` resolves **by name** from any directory:

```Aoxn
import * from "stdlib"

def main() -> int:
    print(isqrt(99))
    return 0
```

`import * from "stdlib/ui_win"` pulls in the UI toolkit; link with
`-l user32 -l gdi32`:

```powershell
aoxn run examples\ui_gallery.ax -l user32 -l gdi32
```

Resolution order for a bare specifier is: a project-local package in
`aox_modules/<name>/`, then the installed stdlib. A local package always
wins. `$AOXN_STDLIB` overrides the stdlib location entirely — that is how to
test an unreleased stdlib.

## Build from source

```powershell
git clone https://github.com/AlonechatWorkspace/Aoxn-language
cd Aoxn-language
cargo build
cargo run -- run examples\hello.ax
```

To produce the installer yourself:

```powershell
powershell -ExecutionPolicy Bypass -File dist\package.ps1
# -> dist\release\Aoxn-<version>-Setup.exe
```

A source checkout also works in place — `cargo run -- run examples\hello.ax`
finds `stdlib/` next to the checkout.

---

## 中文速查

**一个 exe 装全部。** 下载 `Aoxn-<version>-Setup.exe`，双击即可：窗口里有进度条
和日志，装完自动把 `aoxn` 加进 PATH，并跑一次 `aoxn doctor` 自检（真的编译并运行
一个程序），只有通过后按钮才变成 **Finish**。装完请**开一个新的终端**。

默认安装目录 `%LOCALAPPDATA%\aoxn`（可用 `-Prefix <dir>` 或 `%AOXN_HOME%` 改）：

```text
%LOCALAPPDATA%\aoxn\bin\aoxn.exe    编译器
%LOCALAPPDATA%\aoxn\lib\stdlib\    标准库 + UI 工具箱
%LOCALAPPDATA%\aoxn\examples\       示例程序
```

静默安装（脚本 / CI，无窗口）：

```powershell
.\Aoxn-0.30.0-Setup.exe -Console -Prefix C:\Tools\aoxn
```

参数：`-Console` / `-Quiet`（无窗口）、`-Prefix <dir>`（安装目录）、
`-NoClang`（不装 C 工具链）、`-Uninstall`（卸载）、`-?`（帮助）。

**必须另装 clang**（Aoxn 生成 C 交给它编译链接）。安装程序会依次找
`$AOXN_CLANG` → `PATH` → 安装目录下的 `toolchain\bin\clang.exe` →
`C:\Program Files\LLVM\bin`，都没有就用 `winget install LLVM.LLVM` 装 LLVM。
Windows 上**链接**还需要 MSVC Build Tools（clang 会自动探测）：

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools `
  --override "--quiet --add Microsoft.VisualStudio.Workload.VCTools"
```

自检与用法：

```powershell
aoxn doctor            # 打印安装根目录、stdlib、clang，并真的编译运行一个程序
aoxn doctor --json     # 同样的信息，JSON 输出
aoxn run examples\hello.ax
```

装好后任何目录都能直接 `import * from "stdlib"`（不需要相对路径）。
从源码构建并自行打包：`cargo build`，然后
`powershell -ExecutionPolicy Bypass -File dist\package.ps1`。