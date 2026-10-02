# Installing Aoxn

Aoxn ships as a **portable toolchain**: one download, one directory, one
command on `PATH`. No build step, no system-wide registry entries, no
dependency on Rust being installed.

The install layout (identical on all three platforms):

```text
<root>/
  bin/aoxn[.exe]        the compiler driver
  lib/stdlib/*.ax       the standard library + the UI toolkit
  examples/*.ax         runnable samples
  dist/install.sh       the one-click installers (also in the archive)
  docs/install.md       this file
```

`<root>` is `~/.aoxn` (Linux/macOS) or `%LOCALAPPDATA%\aoxn` (Windows) by
default; override it with `--prefix` / `-Prefix` or the `AOXN_HOME`
environment variable.

## One-click install

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/AlonechatWorkspace/Aoxn-language/main/dist/install.ps1 | iex
```

Or download `aoxn-v<version>-windows-x86_64.zip` from the
[releases page](https://github.com/AlonechatWorkspace/Aoxn-language/releases),
unpack it anywhere, and run `bin\aoxn.exe doctor`.

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/AlonechatWorkspace/Aoxn-language/main/dist/install.sh | bash
```

The script detects OS and CPU, downloads
`aoxn-v<version>-<os>-<arch>.tar.gz`, unpacks it into `$AOXN_HOME`, symlinks
`bin/aoxn` into `~/.local/bin`, and finishes with `aoxn doctor`. Useful flags:

| flag | meaning |
| --- | --- |
| `--version <v>` | install a specific version (default: latest release) |
| `--prefix <dir>` | install root (default: `$AOXN_HOME`, else `~/.aoxn`) |
| `--archive <path>` | install from a local archive or unpacked directory (offline) |
| `--from-source` | `cargo install --path .` from a source checkout |
| `--no-clang` | do not touch the C toolchain |
| `--no-path` | do not modify `PATH` |
| `--force` | overwrite an existing install |
| `--uninstall` | remove the install and the PATH entry |

`install.ps1` takes the same flags as `-Version`, `-Prefix`, `-Archive`,
`-NoClang`, `-NoPath`, `-Force`, `-Uninstall`.

## The C toolchain

Aoxn lowers programs to ISO C and hands them to **clang**; the compiler
binary is not self-contained. The installers look for clang in this order:

1. `$AOXN_CLANG` — an explicit path wins over everything else
2. `PATH`
3. `<root>/toolchain/bin/clang` — drop a portable LLVM here to make the
   install self-contained (a plain `clang.exe` alone is enough for compiling;
   the linker still needs its platform libraries)
4. the usual system locations (`C:\Program Files\LLVM\bin` on Windows)

If none matches, the installers try to provide one:

- **Windows**: `winget install LLVM.LLVM`. Linking also needs the **MSVC
  Build Tools** (`winget install Microsoft.VisualStudio.2022.BuildTools` with
  the `Microsoft.VisualStudio.Component.VC.Tools.x86.x64` workload) — clang
  detects them automatically.
- **macOS**: `brew install llvm`, or the Xcode command line tools
  (`xcode-select --install`) which already ship a clang.
- **Linux**: `apt-get install clang`, `dnf install clang`, or
  `pacman -S clang` (only attempted with passwordless `sudo`; otherwise the
  script prints the command to run).

The UI toolkit has one extra dependency on the Unix side: the **X11 backend**
(`stdlib/ui_x11.ax`) needs XQuartz on macOS (`brew install xquartz`) and
`libx11-dev libxft-dev` on Debian/Ubuntu. The plain compiler and the standard
library need nothing beyond clang.

## Verify the install

```bash
aoxn doctor
```

It prints the resolved install root, stdlib location, clang path and version,
then **compiles and runs a one-line program that imports the stdlib** — the
real end-to-end check. Exit code 0 means `aoxn run <file.ax>` will work.

```console
$ aoxn doctor
Aoxn 0.30.0 doctor
  [    ] version      0.30.0
  [    ] platform     linux x86_64 (target_os() = linux)
  [ok  ] install root /home/you/.aoxn
  [ok  ] stdlib       /home/you/.aoxn/lib/stdlib
  [    ] examples     /home/you/.aoxn/examples (18 .ax files)
  [ok  ] clang        /usr/bin/clang  clang version 18.1.8
  [ok  ] smoke test   ok  (aoxn-doctor-ok)
  [    ] cache dir    /work/target/cache

status: ok — `aoxn run <file.ax>` is ready to use.
```

`aoxn doctor --json` emits the same information as JSON for scripts and
agents; `--no-smoke` skips the compile+run step.

## Using the standard library

With a release install, `stdlib` resolves **by name** from any directory:

```python
import * from "stdlib"

def main() -> int:
    print(isqrt(99))
    return 0
```

`import * from "stdlib/ui"` pulls in the UI toolkit (add the backend's link
flags: `-l user32 -l gdi32` on Windows, `-l X11 -l Xft` on Linux/macOS).

Resolution order for a bare specifier is: a local package in
`aox_modules/<name>/`, then the installed stdlib. A project-local package
always wins. `AOXN_STDLIB` overrides the stdlib location entirely, which is
what you want when testing an unreleased stdlib.

## Building from source

```bash
git clone https://github.com/AlonechatWorkspace/Aoxn-language
cd Aoxn-language
cargo install --path .          # or: dist/install.sh --from-source
```

A source checkout also works in place — `cargo run -- run examples/hello.ax`
resolves `stdlib/` next to the checkout. Cloning with submodules is not
required; the repository has no submodules.

## Uninstalling

```bash
dist/install.sh --uninstall          # Linux/macOS
.\dist\install.ps1 -Uninstall        # Windows
```

Remove the directory and the PATH entry; no other state is touched. The
package manager's cache lives in `$AOXN_HOME` (or `~/.aoxn`) and the build
cache in `./target/cache` of whatever project you compile in.

---

## 中文速查

三平台安装方式相同：下载一个压缩包，解压即用。

```powershell
# Windows
irm https://raw.githubusercontent.com/AlonechatWorkspace/Aoxn-language/main/dist/install.ps1 | iex
```

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/AlonechatWorkspace/Aoxn-language/main/dist/install.sh | bash
```

安装目录默认 `~/.aoxn`（Windows 为 `%LOCALAPPDATA%\aoxn`），可用
`--prefix` / `-Prefix` 或 `AOXN_HOME` 改变。目录结构：

```text
<root>/bin/aoxn[.exe]   编译器
<root>/lib/stdlib/*.ax  标准库 + UI 工具包
<root>/examples/*.ax    示例程序
```

**必须另装 clang**（Aoxn 生成 C 代码后交给 clang 编译链接）：
Windows 用 `winget install LLVM.LLVM`（链接还需要 MSVC Build Tools）；
macOS 用 `brew install llvm`；Linux 用 `apt-get install clang`。
也可以把便携版 LLVM 放进 `<root>/toolchain/bin/`，或用 `AOXN_CLANG` 指定路径。

装完跑一次自检：

```bash
aoxn doctor          # 打印安装根目录、stdlib、clang，并真的编译运行一个程序
aoxn doctor --json   # 同样的信息，JSON 输出
```

装好后任何目录都能直接 `import * from "stdlib"`（不再需要相对路径）。
卸载：`dist/install.sh --uninstall`。