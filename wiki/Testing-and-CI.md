# 测试与 CI · Testing & CI

> **中文**：`tests/pipeline.rs` 是 Aoxn 的真源测试套件——97 个端到端测试把 `.ax` 编译成可执行文件、运行它、
> 断言 stdout 与退出码；本页说明测试哲学、测试 helper、主题分组、运行方式、`ci.yml` 四平台矩阵与 `web-bench.yml`
> 的真实步骤，以及本地与 CI 的差异和排错路径。
> **English**: `tests/pipeline.rs` is Aoxn's source-of-truth suite — 97 end-to-end tests that compile `.ax` to an
> executable, run it, and assert stdout plus exit code; this page covers the testing philosophy, the helpers, the
> thematic grouping, how to run the suite, the real steps of the four-platform `ci.yml` matrix and `web-bench.yml`,
> plus local-vs-CI differences and a troubleshooting path.

## 中文

### 测试哲学

`tests/pipeline.rs` 是真源（source of truth）。多数测试走完整流水线：把 `.ax` 源码编译成原生可执行文件，
启动它，然后断言它的 **stdout 与退出码**；拒绝类用例只断言编译失败。没有 mock、没有只测内部函数的单元测试——
一个测试通过，意味着
“这段源码能被编译、能链接、能运行、输出正确”，也就是用户真正关心的那件事。

由此推论出的三条纪律：

1. **测试名读起来应该像它守护的不变式**，而不是像它调用的函数：`optimization_levels_agree_on_program_output`
   （优化级别不能改变程序行为）、`selfhost_driver_links_hello`（自举驱动能链出可运行的 exe）、
   `rejects_missing_return`（缺返回值的函数必须被拒绝）。读测试名就能知道什么坏了。
2. **严格语义让“拒绝”也可测**：类型错误不是崩溃而是诊断，所以约三分之一的测试断言编译**必须失败**，并检查
   第一条诊断消息（`expect_compile_error`）。
3. **改可观察行为就要加测试**（这条来自 [CONTRIBUTING.md](../CONTRIBUTING.md)）。

产物落在系统临时目录：普通测试用 `%TEMP%\Aoxn-tests`（`t{id}.exe` 之类，`id` = 进程内计数器 + PID，
保证并行测试不撞名），import 测试用 `%TEMP%\Aoxn-import-{tag}-{id}`。编译类测试跑完会删掉自己产出的
exe/obj，但 `tmp_dir()` 建的 `%TEMP%\Aoxn-import-*` fixture 目录**不会**被删除（`remove_dir_all` 只出现在
自举测试与平台探测测试里，import 用例没有清理这一步）。

### 测试基础设施（真实 helper 清单）

文件顶部（前 ~90 行）与各段落之间定义了一组 helper。签名与行为如下：

| helper | 签名 | 行为 |
|---|---|---|
| `stdlib_src` | `fn stdlib_src() -> String` | 从 `CARGO_MANIFEST_DIR` 读 `stdlib/stdlib.ax`，缺失则 panic（`stdlib/stdlib.ax not found`） |
| `build_and_run_with_stdlib` | `fn build_and_run_with_stdlib(src: &str) -> String` | 把 stdlib 源码文本前置拼到去缩进后的测试源码上，再交给 `build_and_run`（同一编译单元，不是 `import`） |
| `dedent` | `fn dedent(src: &str) -> String` | 去掉公共前导缩进与开头空行、丢弃末尾空行、结尾恰好一个换行——测试因此可以在 Rust 代码里保持缩进书写 |
| `build_and_run` | `fn build_and_run(src: &str) -> String` | `dedent` → `aoxn::build_exe(src, &exe, true)`（`true` = O3）→ 运行 → 断言退出码成功 → 返回 stdout；编译失败则 panic 并打印全部诊断 |
| `expect_compile_error` | `fn expect_compile_error(src: &str) -> String` | 断言编译**失败**，返回第一条诊断的 `message`（编译成功则 panic） |
| `build_and_run_lvl` | `fn build_and_run_lvl(src: &str, opt_level: u8) -> String` | 与 `build_and_run` 相同，但走优化级别 API `aoxn::build_exe_lvl`，产物名带级别前缀 |
| `tmp_dir` | `fn tmp_dir(tag: &str) -> PathBuf` | 建 `%TEMP%\Aoxn-import-{tag}-{id}` 并返回，供多文件 import 测试写 fixture 树 |
| `EXE` | `const EXE: &str` | 平台可执行文件后缀（Windows `.exe`，其它平台空），等价于 `platform::exe_ext()` 的带点形式 |
| `llvm_link_name` | `fn llvm_link_name() -> String` | 转发到 `aoxn::platform::llvm_link_name()`——**不**硬编码 `LLVM-C`（Linux 上那是 `libLLVM-<N>`） |
| `path_with_llvm_bin` | `fn path_with_llvm_bin(llvm: &std::path::Path) -> String` | 用 `std::env::join_paths` 把 `<llvm>/bin` 前置到 PATH（POSIX 上用 `;` 拼接会把整条 PATH 压成一个不存在的目录） |
| `llvm_dir` | `fn llvm_dir() -> Option<PathBuf>` | 按 `AOXN_LLVM_DIR` / `AXON_LLVM_DIR` → `<repo>/LLVM` → `C:/Program Files/LLVM` 顺序找 LLVM 安装 |

有一段注释点明了 `llvm_link_name` 的意图：“Windows/macOS 有专门的 `LLVM-C`；Debian/Ubuntu 把 C API 放进版本化的
`libLLVM-<N>.so`”。库名探测规则本身由测试 `llvm_link_name_probe_covers_platform_layouts` 在临时目录里直接验证，
不依赖宿主布局。

### 97 个测试的主题分组

下表是本文档作者按主题划分的归类（计数与文件里 `#[test]` 的总数 97 对齐，可自行用
`Select-String '^#\[test\]' tests/pipeline.rs` 核对）：

| # | 主题 | 个数 | 代表测试 |
|---|---|---|---|
| 1 | 语言核心、算术、函数与对比 | 11 | `hello_world`、`arithmetic_and_precedence`、`recursion_fib`、`mutual_recursion`、`comparison_boundary_cases`、`exit_code_propagates` |
| 2 | 控制流与布尔短路 | 3 | `while_loop_and_mutation`、`elif_else_chains`、`bool_logic_and_short_circuit` |
| 3 | 严格类型与程序结构的拒绝 | 11 | `rejects_int_float_mixing`、`rejects_missing_return`、`rejects_unreachable_code`、`rejects_missing_main`、`rejects_bad_indentation`、`rejects_top_level_statement` |
| 4 | 数组与嵌套聚合 | 4 | `array_literal_index_len`、`array_value_semantics`、`nested_arrays` |
| 5 | 结构体与值语义 | 7 | `struct_basic`、`struct_value_semantics`、`struct_params_and_return`、`struct_with_array_field`、`array_of_structs` |
| 6 | 聚合类型、字段与索引的拒绝 | 8 | `rejects_print_struct`、`rejects_unknown_struct_field`、`rejects_recursive_struct`、`rejects_zero_len_array`、`rejects_struct_kwarg_on_fn` |
| 7 | 字符串与 f-string | 10 | `string_concat`、`string_ordering`、`strings_in_structs_and_arrays`、`string_concat_in_loop`、`f_string_basics`、`str_builtin`、`rejects_concat_string_int` |
| 8 | 循环与 `break` / `continue` | 4 | `for_range_forms`、`for_array_iteration`、`for_nested_and_reuse`、`break_and_continue` |
| 9 | 循环 / range / f-string 的拒绝 | 5 | `rejects_break_outside_loop`、`rejects_range_float_arg`、`rejects_for_over_int`、`rejects_fstring_of_array` |
| 10 | 泛型与 stdlib 算法 | 4 | `stdlib_math`、`stdlib_generic_sort_search`、`stdlib_sort_does_not_mutate`、`nested_generic_calls` |
| 11 | 泛型与入口签名的拒绝 | 5 | `rejects_generic_struct_sort`、`rejects_extern_main`、`rejects_generic_main`、`rejects_uninferable_len` |
| 12 | `import` 与多文件 | 6 | `import_transitive_and_include_once`、`import_aggregate_literals_across_files`、`import_cycle_detected`、`import_error_reports_importing_file`、`string_sources_reject_imports` |
| 13 | stdlib `Vec`、raw memory、文件 IO、进程 | 5 | `stdlib_vec_grow_and_slots`、`infer_binding_from_as_string_and_as_ptr`、`stdlib_file_io_roundtrip`、`stdlib_system_spawn` |
| 14 | 自举各阶段（lexer → parser → typecheck → codegen → driver → 固定点） | 10 | `selfhost_lexer_token_stream`、`selfhost_parser_ast_dump`、`selfhost_typechecker_accepts_and_rejects`、`selfhost_codegen_int_slice`、`selfhost_driver_links_hello`、`selfhost_driver_self_compiles` |
| 15 | 优化级别、依赖文件、平台库名探测 | 4 | `optimization_levels_agree_on_program_output`、`optimization_levels_produce_distinct_ir`、`dependency_files_follows_import_chain`、`llvm_link_name_probe_covers_platform_layouts` |

第 14 组的后半段（codegen / driver / 固定点）需要 LLVM 安装与 clang，用 `llvm_dir()` 定位、把库名交给
`-l`；其中 `selfhost_driver_self_compiles` 以 `C:\Program Files\LLVM\lib\LLVM-C.lib` 存在为运行前提，
非 Windows 上会自跳过（见“本地与 CI 的差异”）。

### 运行方式

```powershell
cargo test                                     # 全量：97 个端到端测试
cargo test --test pipeline recursion_fib       # 单个测试（测试名即过滤器）
cargo test --test pipeline rejects_            # 一类测试：所有 rejects_* 拒绝用例
```

```bash
cargo test
cargo test --test pipeline recursion_fib
cargo test --test pipeline rejects_
```

两点注意：

- **编译型测试天生慢**：每个用例都要跑一次完整编译 + 链接 + 启动进程。迭代大输入时可以加 `--O1`
  （`cargo run -- run big.ax --O1` / `aoxn build --O1`）把编译时间近乎减半；但 **O3 是默认级别，也是
  “与 `clang -O3` 同性能”这句承诺所在的级别**，因此性能验证与 CI 都跑 O3。
- 提交前 `cargo fmt`，并保持 warning 数为零（CONTRIBUTING.md 的代码约定）。

### 新增测试的规则（CONTRIBUTING.md）

- **改可观察行为就要加测试**，名字写成它守护的不变式（`optimization_levels_agree_on_program_output`、
  `selfhost_driver_links_hello` 就是范例）。
- **不要为了让改动通过而弱化或删除既有测试**。如果某个测试编码的是过时行为，就**有意**改它并在 PR 里说明。
- 内嵌的 Aoxn 源码走 `dedent()`；优先用 `src/platform.rs` 的 helper，而不是 `LLVM-C`、`.exe` 这类字面量
  （老测试里仍有硬编码，**不要新增同类**）；测试套件必须在 Windows、Linux、macOS 上都过。
- **fixture 用 ASCII**：自举 loader 用窄字符 `fopen` 打开文件，非 ASCII 路径在自举路径上会失败。
- 内嵌源码同样**不要写 UTF-8 BOM**：Aoxn 按字节读源码，`EF BB BF` 会在 1:1 报 “unexpected character”
  （PowerShell 5.1 的 `-Encoding UTF8` 会加 BOM，用编辑器或 Write 工具写文件）。

### CI 矩阵：`.github/workflows/ci.yml`

触发条件：push 到 `main`，以及所有 pull request。四个 job 结构一致：
**checkout → 缓存 cargo → 安装 LLVM → `cargo build` → `cargo test` → smoke test**。

| job | runner | Tier | LLVM 安装 | `AOXN_LLVM_DIR` | smoke |
|---|---|---|---|---|---|
| `windows` | `windows-latest` | 1 | `winget install LLVM.LLVM`，`C:\Program Files\LLVM\bin` 追加到 `GITHUB_PATH` | `C:\Program Files\LLVM`（job 级 env） | hello + strings + primes 断言 9592 |
| `linux` | `ubuntu-latest` | 2 | `apt-get install llvm-18-dev clang-18 libclang-rt-18-dev` + clang 软链 | `/usr/lib/llvm-18`（build/test/smoke 各自 env） | hello + strings + primes 断言 9592 |
| `macos-x86_64` | `macos-13` | 2 | `brew install llvm@18` | `/usr/local/opt/llvm@18` | hello + strings |
| `macos-arm64` | `macos-14` | 2 | `brew install llvm@18` | `/opt/homebrew/opt/llvm@18` | hello + strings |

缓存步骤用 `actions/cache@v4` 缓存 `~/.cargo/bin`、`~/.cargo/registry`、`~/.cargo/git` 与 `target`，
key 为 `${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}`（两个 macOS job 的 key 还带
`${{ runner.arch }}`，避免 Intel 与 arm64 互抢缓存）。

Windows job 的真实内容（Tier 1）：

```yaml
  windows:
    runs-on: windows-latest
    env:
      AOXN_LLVM_DIR: "C:\\Program Files\\LLVM"
    steps:
      - uses: actions/checkout@v4
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: ${{ runner.os }}-cargo-
      - name: Install LLVM
        shell: pwsh
        run: |
          winget install --id LLVM.LLVM --accept-source-agreements --accept-package-agreements --silent
          Add-Content $env:GITHUB_PATH "C:\Program Files\LLVM\bin"
      - name: Build
        run: cargo build
      - name: Test (97 end-to-end pipeline tests)
        run: cargo test
      - name: Smoke test
        shell: pwsh
        run: |
          cargo run -- run examples\hello.ax
          cargo run -- run examples\strings.ax
          cargo run -- build examples\primes.ax -o primes.exe
          $count = .\primes.exe
          if ("$count" -ne "9592") { throw "primes returned $count, expected 9592" }
```

Windows 的 smoke test 断言（注意 `primes.ax` 统计 10 万以内的素数个数，期望 `9592`）：

```powershell
cargo run -- run examples\hello.ax
cargo run -- run examples\strings.ax
cargo run -- build examples\primes.ax -o primes.exe
$count = .\primes.exe
if ("$count" -ne "9592") { throw "primes returned $count, expected 9592" }
```

Linux 的安装与 smoke（没有扩展名，断言方式也换成 shell）：

```yaml
      - name: Install LLVM
        run: |
          sudo apt-get update
          sudo apt-get install -y llvm-18-dev clang-18 libclang-rt-18-dev
          # make the versioned tools discoverable for the compiler's clang lookup
          sudo ln -sf /usr/lib/llvm-18/bin/clang /usr/local/bin/clang || true
      - name: Smoke test
        env:
          AOXN_LLVM_DIR: /usr/lib/llvm-18
        run: |
          cargo run -- run examples/hello.ax
          cargo run -- run examples/strings.ax
          cargo run -- build examples/primes.ax -o primes
          count=$(./primes)
          if [ "$count" != "9592" ]; then echo "primes returned $count, expected 9592"; exit 1; fi
```

两个 macOS job 的差异只在路径与 runner（brew 的 llvm 是 keg-only，`bin` 不在 PATH 上，所以 `AOXN_LLVM_DIR`
是必需的），并且它们的 smoke 只跑 hello 与 strings —— **没有 primes 断言**：

```yaml
  macos-arm64:
    runs-on: macos-14
    steps:
      # ... checkout + cache（key 带 runner.arch）
      - name: Install LLVM
        run: brew install llvm@18
      - name: Build
        env:
          AOXN_LLVM_DIR: /opt/homebrew/opt/llvm@18
        run: cargo build
      - name: Test
        env:
          AOXN_LLVM_DIR: /opt/homebrew/opt/llvm@18
        run: cargo test
      - name: Smoke test
        env:
          AOXN_LLVM_DIR: /opt/homebrew/opt/llvm@18
        run: |
          cargo run -- run examples/hello.ax
          cargo run -- run examples/strings.ax
```

（`macos-x86_64` 与它逐字对应，只是 `runs-on: macos-13`、路径为 `/usr/local/opt/llvm@18`。）

### Web 基准：`.github/workflows/web-bench.yml`

这个 workflow 覆盖 `web/` 那一套（Aoxn 写的 HTTP/1.1 服务器 vs Node/Next.js 对照）：

- 触发：`workflow_dispatch`，以及改动 `web/**` 或该 workflow 文件本身的 push / PR；
- 单个 job `web`，三平台 matrix（`fail-fast: false`）：
  `windows-latest`（`entry: web/server_win.ax`、`out: web/server.exe`、`extra_flags: -l ws2_32`）、
  `ubuntu-latest`（`/usr/lib/llvm-18`，`server_posix.ax` → `web/server`）、
  `macos-14`（`/opt/homebrew/opt/llvm@18`，同上）；env 为 `AOXN_LLVM_DIR` 与 `NEXT_TELEMETRY_DISABLED=1`；
- 步骤：checkout → 缓存 cargo（key 前缀 `-web-cargo-`，带 `runner.arch`）→ 按 OS 装 LLVM →
  `cargo build --release` → `cargo run --release -- build <entry> -o <out> <extra_flags>` →
  `actions/setup-node@v4`（Node 22）→ `node web/loadtest/parity.mjs <out>`（功能对等：
  与 Node 参考实现的 body 一致、404、keep-alive、`/metrics`）→ 在 `web/next-app` 里
  `corepack enable` / `pnpm install --frozen-lockfile` / `pnpm build` → 按 OS 下载 oha v1.16.0 →
  `BENCH_TRIALS=1 BENCH_DURATION=5s node web/loadtest/bench.mjs`（短基准；功能对等由它前面独立的
  `parity.mjs` 步骤负责）→
  上传 `web/loadtest/last-results.json` 为 artifact `web-bench-<os>`；
- **runner 上的数字只作趋势**（文件头注释写明 CI runner 共享 CPU，数值不是绝对值），细节见
  [Web 平台](Web-Platform.md)。

### 本地与 CI 的差异

| 项目 | 本地（开发机） | CI |
|---|---|---|
| LLVM 版本 | 23.1.0（winget 安装） | Windows job 用 winget 装最新版；Tier 2 三个 job 固定 LLVM 18（`llvm-18-dev` / `llvm@18`） |
| 自举固定点 `selfhost_driver_self_compiles` | 真跑（Windows，IR 与目标文件逐字节比较） | Windows job 真跑；**Linux/macOS job 自跳过**（要求 `C:\Program Files\LLVM\lib\LLVM-C.lib` 存在，否则打印 `skipping: standard LLVM install not found` 后直接返回） |
| 构建缓存 | 本机 `target/` + `target/cache` | `actions/cache` 缓存 `~/.cargo/*` 与 `target`，按 `Cargo.lock` 哈希失效 |
| smoke 覆盖 | 手动执行 | 每个 job 都跑；只有 Windows 与 Linux 断言 primes = 9592 |

因此“本地全绿”与“CI 全绿”不是同一件事：非 Windows 平台上，自举固定点的逐字节验证没有执行（其余 selfhost
测试照常跑）。这条边界在 [平台支持](Platform-Support.md) 里也有记录。

### 排错指引：某个测试失败时先看什么

1. **先看阶段**：`AOXN_TIME=1` 会打印 lex / parse / typecheck / codegen / link 各阶段的 wall-clock，以及
   codegen 的子阶段（`cg.build`、`cg.verify`、`cg.target`、`cg.passes`、`cg.isel`）。失败落在哪个阶段，
   基本就定位到哪个模块。
2. **看 IR**：`AOXN_DUMP_IR=1` 把 verify 之前的 IR 打到 stderr；想看优化后的 IR 用 `cargo run -- ir file.ax`。
   IR 层面的差异也会直接让自举固定点测试报 “stage-1 and stage-2 IR differ”。
3. **要机器可读诊断**：`cargo run -- run bad.ax --json` 输出
   `{"ok":false,"errors":[{"stage","file","line","col","message"}]}`，阶段取值为
   `lex | parse | type | internal | link | io`。
4. **“编译产物找不到”多半是平台扩展名问题**：非 Windows 上可执行文件没有扩展名、目标文件是 `.o` 而不是
   `.obj`。测试里用 `EXE` 常量与 `platform::exe_ext()` / `platform::obj_ext()`，自举 demo 用
   `driver.ax` 的 `exe_suffix()`。
5. **自举测试找不到 LLVM**：设置 `AOXN_LLVM_DIR`；Linux 上 `cannot find -lLLVM-C` 属预期行为，正确做法是让
   代码走 `platform::llvm_link_name()`（或在特殊情况下用 `AOXN_LLVM_LIB` 覆盖）。
6. **`Cannot choose between targets`**：LLVM 的目标注册是进程全局的，同一个后端注册两次就会这样——保持
   `init_target()` 的 `Once` 语义。
7. **1:1 `unexpected character`**：编辑 `.ax` fixture 时写进了 UTF-8 BOM（PowerShell 5.1 的
   `-Encoding UTF8` 是常见来源），按字节剥掉前三个字节即可。

## English

### Testing philosophy

`tests/pipeline.rs` is the source of truth. Most tests drive the whole pipeline: they compile `.ax` source into a
native executable, launch it, and assert its **stdout and exit code**; rejection cases only assert that compilation
fails. There are no mocks and no unit tests that poke internal functions — a passing test means "this source
compiles, links, runs, and prints the right thing", which is what users actually care about.

Three rules follow from that:

1. **A test name should read like the invariant it protects**, not like the function it calls:
   `optimization_levels_agree_on_program_output` (the optimization level must not change program behavior),
   `selfhost_driver_links_hello` (the self-hosted driver links a runnable exe),
   `rejects_missing_return` (a function
   without a return must be rejected). Reading the name tells you what broke.
2. **Strict semantics make rejection testable**: a type error is a diagnostic, not a crash, so
   roughly a third of the tests assert that compilation **must fail** and inspect the first diagnostic
   message (`expect_compile_error`).
3. **Any observable behavior change comes with a test** (this rule comes from
   [CONTRIBUTING.md](../CONTRIBUTING.md)).

Artifacts land in the system temporary directory: ordinary tests use `%TEMP%\Aoxn-tests` (`t{id}.exe` and friends,
where `id` is an in-process counter plus the PID, so parallel tests never collide) and the import tests use
`%TEMP%\Aoxn-import-{tag}-{id}`. Compiling tests delete the exe/obj they produced when they finish, but the
`%TEMP%\Aoxn-import-*` fixture directories created by `tmp_dir()` are **not** removed (`remove_dir_all` appears only
in the selfhost and platform-probe tests; the import cases have no cleanup step).

### Test infrastructure (the real helper list)

A set of helpers lives at the top of the file (first ~90 lines) and between sections. Signatures and behavior:

| Helper | Signature | Behavior |
|---|---|---|
| `stdlib_src` | `fn stdlib_src() -> String` | reads `stdlib/stdlib.ax` from `CARGO_MANIFEST_DIR`, panicking with `stdlib/stdlib.ax not found` if missing |
| `build_and_run_with_stdlib` | `fn build_and_run_with_stdlib(src: &str) -> String` | prepends the stdlib source text to the dedented test source and routes it through `build_and_run` (one compilation unit, not an `import`) |
| `dedent` | `fn dedent(src: &str) -> String` | strips the common leading indentation and leading blank lines, drops trailing blank lines, and ends with exactly one newline — so tests can stay indented inside Rust code |
| `build_and_run` | `fn build_and_run(src: &str) -> String` | `dedent` → `aoxn::build_exe(src, &exe, true)` (`true` = O3) → run → assert a successful exit code → return stdout; a failed compile panics with all diagnostics |
| `expect_compile_error` | `fn expect_compile_error(src: &str) -> String` | asserts that compilation **fails** and returns the first diagnostic's `message` (it panics if compilation succeeds) |
| `build_and_run_lvl` | `fn build_and_run_lvl(src: &str, opt_level: u8) -> String` | same as `build_and_run` but through the optimization-level API `aoxn::build_exe_lvl`, with the level in the artifact name |
| `tmp_dir` | `fn tmp_dir(tag: &str) -> PathBuf` | creates and returns `%TEMP%\Aoxn-import-{tag}-{id}` for multi-file import fixtures |
| `EXE` | `const EXE: &str` | platform executable suffix (`.exe` on Windows, empty elsewhere), the dotted equivalent of `platform::exe_ext()` |
| `llvm_link_name` | `fn llvm_link_name() -> String` | forwards to `aoxn::platform::llvm_link_name()` — it does **not** hardcode `LLVM-C` (that name does not exist on Linux, where it is `libLLVM-<N>`) |
| `path_with_llvm_bin` | `fn path_with_llvm_bin(llvm: &std::path::Path) -> String` | prepends `<llvm>/bin` to PATH via `std::env::join_paths` (joining with `;` on POSIX collapses the whole PATH into one nonexistent directory) |
| `llvm_dir` | `fn llvm_dir() -> Option<PathBuf>` | locates the LLVM install as `AOXN_LLVM_DIR` / `AXON_LLVM_DIR` → `<repo>/LLVM` → `C:/Program Files/LLVM` |

A comment states the intent behind `llvm_link_name`: "Windows/macOS ship `LLVM-C`; Debian/Ubuntu
LLVM puts the C API
in versioned `libLLVM-<N>.so`". The probing rules themselves are verified by
`llvm_link_name_probe_covers_platform_layouts` over temporary directories, independent of the host layout.

### Thematic grouping of the 97 tests

The table below is this document's thematic grouping (its counts add up to the 97 `#[test]`s in the
file; verify with
`Select-String '^#\[test\]' tests/pipeline.rs`):

| # | Theme | Count | Representative tests |
|---|---|---|---|
| 1 | Language core, arithmetic, functions, comparisons | 11 | `hello_world`, `arithmetic_and_precedence`, `recursion_fib`, `mutual_recursion`, `comparison_boundary_cases`, `exit_code_propagates` |
| 2 | Control flow and boolean short-circuiting | 3 | `while_loop_and_mutation`, `elif_else_chains`, `bool_logic_and_short_circuit` |
| 3 | Strict typing and program-structure rejections | 11 | `rejects_int_float_mixing`, `rejects_missing_return`, `rejects_unreachable_code`, `rejects_missing_main`, `rejects_bad_indentation`, `rejects_top_level_statement` |
| 4 | Arrays and nested aggregates | 4 | `array_literal_index_len`, `array_value_semantics`, `nested_arrays` |
| 5 | Structs and value semantics | 7 | `struct_basic`, `struct_value_semantics`, `struct_params_and_return`, `struct_with_array_field`, `array_of_structs` |
| 6 | Aggregate type, field and index rejections | 8 | `rejects_print_struct`, `rejects_unknown_struct_field`, `rejects_recursive_struct`, `rejects_zero_len_array`, `rejects_struct_kwarg_on_fn` |
| 7 | Strings and f-strings | 10 | `string_concat`, `string_ordering`, `strings_in_structs_and_arrays`, `string_concat_in_loop`, `f_string_basics`, `str_builtin`, `rejects_concat_string_int` |
| 8 | Loops, `break` / `continue` | 4 | `for_range_forms`, `for_array_iteration`, `for_nested_and_reuse`, `break_and_continue` |
| 9 | Loop / range / f-string rejections | 5 | `rejects_break_outside_loop`, `rejects_range_float_arg`, `rejects_for_over_int`, `rejects_fstring_of_array` |
| 10 | Generics and stdlib algorithms | 4 | `stdlib_math`, `stdlib_generic_sort_search`, `stdlib_sort_does_not_mutate`, `nested_generic_calls` |
| 11 | Generic and entry-signature rejections | 5 | `rejects_generic_struct_sort`, `rejects_extern_main`, `rejects_generic_main`, `rejects_uninferable_len` |
| 12 | `import` and multi-file programs | 6 | `import_transitive_and_include_once`, `import_aggregate_literals_across_files`, `import_cycle_detected`, `import_error_reports_importing_file`, `string_sources_reject_imports` |
| 13 | stdlib `Vec`, raw memory, file IO, processes | 5 | `stdlib_vec_grow_and_slots`, `infer_binding_from_as_string_and_as_ptr`, `stdlib_file_io_roundtrip`, `stdlib_system_spawn` |
| 14 | Self-hosting stages (lexer → parser → typecheck → codegen → driver → fixed point) | 10 | `selfhost_lexer_token_stream`, `selfhost_parser_ast_dump`, `selfhost_typechecker_accepts_and_rejects`, `selfhost_codegen_int_slice`, `selfhost_driver_links_hello`, `selfhost_driver_self_compiles` |
| 15 | Optimization levels, dependency files, platform link-name probing | 4 | `optimization_levels_agree_on_program_output`, `optimization_levels_produce_distinct_ir`, `dependency_files_follows_import_chain`, `llvm_link_name_probe_covers_platform_layouts` |

The back half of group 14 (codegen / driver / fixed point) needs an LLVM install plus clang: it locates the install
with `llvm_dir()` and passes the probed name to `-l`. `selfhost_driver_self_compiles` additionally requires
`C:\Program Files\LLVM\lib\LLVM-C.lib` and self-skips off Windows (see "Local vs CI differences").

### Running the suite

```powershell
cargo test                                     # everything: 97 end-to-end tests
cargo test --test pipeline recursion_fib       # one test (the test name is the filter)
cargo test --test pipeline rejects_            # a family: every rejects_* case
```

```bash
cargo test
cargo test --test pipeline recursion_fib
cargo test --test pipeline rejects_
```

Two caveats:

- **Compiling tests are inherently slow**: every case pays a full compile + link + process start.
  While iterating on
  a large input, `--O1` roughly halves compile time (`cargo run -- run big.ax --O1` / `aoxn build
  --O1`); but **O3 is
  the default level and the level the "parity with `clang -O3`" promise refers to**, so performance
  verification and
  CI both run O3.
- Run `cargo fmt` before committing and keep the warning count at zero (CONTRIBUTING.md code conventions).

### Rules for adding tests (CONTRIBUTING.md)

- **Add a test whenever you change observable behavior**, and name it like the invariant it protects
  (`optimization_levels_agree_on_program_output` and `selfhost_driver_links_hello` are the models).
- **Never weaken or delete an existing test to make a change fit.** If a test encodes outdated behavior, change it
  deliberately and say so in the PR.
- Route embedded Aoxn sources through `dedent()`, and prefer the `src/platform.rs` helpers over literals such as
  `LLVM-C` or `.exe` (older tests still hardcode them — **do not add more of the same**); the suite must pass on
  Windows, Linux and macOS.
- **Keep fixtures ASCII**: the self-hosted loader opens files with narrow `fopen`, so non-ASCII paths fail on the
  self-hosted path.
- Embedded sources must also carry **no UTF-8 BOM**: Aoxn reads sources byte-wise, so `EF BB BF` produces
  "unexpected character" at 1:1 (PowerShell 5.1's `-Encoding UTF8` adds one — write files with an editor or the
  Write tool).

### The CI matrix: `.github/workflows/ci.yml`

Triggers: pushes to `main` and every pull request. All four jobs share the same shape:
**checkout → cache cargo → install LLVM → `cargo build` → `cargo test` → smoke test**.

| Job | Runner | Tier | LLVM install | `AOXN_LLVM_DIR` | Smoke |
|---|---|---|---|---|---|
| `windows` | `windows-latest` | 1 | `winget install LLVM.LLVM`, with `C:\Program Files\LLVM\bin` appended to `GITHUB_PATH` | `C:\Program Files\LLVM` (job-level env) | hello + strings + primes asserting 9592 |
| `linux` | `ubuntu-latest` | 2 | `apt-get install llvm-18-dev clang-18 libclang-rt-18-dev` + a clang symlink | `/usr/lib/llvm-18` (per-step env on build/test/smoke) | hello + strings + primes asserting 9592 |
| `macos-x86_64` | `macos-13` | 2 | `brew install llvm@18` | `/usr/local/opt/llvm@18` | hello + strings |
| `macos-arm64` | `macos-14` | 2 | `brew install llvm@18` | `/opt/homebrew/opt/llvm@18` | hello + strings |

The cache step uses `actions/cache@v4` over `~/.cargo/bin`, `~/.cargo/registry`, `~/.cargo/git` and
`target`, keyed on
`${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}` (both macOS jobs also include `${{
runner.arch }}` so Intel
and arm64 do not fight over one cache).

The real Windows job (Tier 1):

```yaml
  windows:
    runs-on: windows-latest
    env:
      AOXN_LLVM_DIR: "C:\\Program Files\\LLVM"
    steps:
      - uses: actions/checkout@v4
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: ${{ runner.os }}-cargo-
      - name: Install LLVM
        shell: pwsh
        run: |
          winget install --id LLVM.LLVM --accept-source-agreements --accept-package-agreements --silent
          Add-Content $env:GITHUB_PATH "C:\Program Files\LLVM\bin"
      - name: Build
        run: cargo build
      - name: Test (97 end-to-end pipeline tests)
        run: cargo test
      - name: Smoke test
        shell: pwsh
        run: |
          cargo run -- run examples\hello.ax
          cargo run -- run examples\strings.ax
          cargo run -- build examples\primes.ax -o primes.exe
          $count = .\primes.exe
          if ("$count" -ne "9592") { throw "primes returned $count, expected 9592" }
```

The Windows smoke assertions (`primes.ax` counts the primes below 100000, hence the expected `9592`):

```powershell
cargo run -- run examples\hello.ax
cargo run -- run examples\strings.ax
cargo run -- build examples\primes.ax -o primes.exe
$count = .\primes.exe
if ("$count" -ne "9592") { throw "primes returned $count, expected 9592" }
```

Linux's install and smoke steps (no extension on the artifact, and a shell assertion instead):

```yaml
      - name: Install LLVM
        run: |
          sudo apt-get update
          sudo apt-get install -y llvm-18-dev clang-18 libclang-rt-18-dev
          # make the versioned tools discoverable for the compiler's clang lookup
          sudo ln -sf /usr/lib/llvm-18/bin/clang /usr/local/bin/clang || true
      - name: Smoke test
        env:
          AOXN_LLVM_DIR: /usr/lib/llvm-18
        run: |
          cargo run -- run examples/hello.ax
          cargo run -- run examples/strings.ax
          cargo run -- build examples/primes.ax -o primes
          count=$(./primes)
          if [ "$count" != "9592" ]; then echo "primes returned $count, expected 9592"; exit 1; fi
```

The two macOS jobs differ only in runner and paths (brew's llvm is keg-only and not on PATH, which is why
`AOXN_LLVM_DIR` is mandatory there), and their smoke step runs only hello and strings — **there is no primes
assertion**:

```yaml
  macos-arm64:
    runs-on: macos-14
    steps:
      # ... checkout + cache (key includes runner.arch)
      - name: Install LLVM
        run: brew install llvm@18
      - name: Build
        env:
          AOXN_LLVM_DIR: /opt/homebrew/opt/llvm@18
        run: cargo build
      - name: Test
        env:
          AOXN_LLVM_DIR: /opt/homebrew/opt/llvm@18
        run: cargo test
      - name: Smoke test
        env:
          AOXN_LLVM_DIR: /opt/homebrew/opt/llvm@18
        run: |
          cargo run -- run examples/hello.ax
          cargo run -- run examples/strings.ax
```

(`macos-x86_64` mirrors it line for line with `runs-on: macos-13` and `/usr/local/opt/llvm@18`.)

### Web benchmarks: `.github/workflows/web-bench.yml`

This workflow covers the `web/` suite (the HTTP/1.1 server written in Aoxn versus the Node/Next.js references):

- triggers: `workflow_dispatch`, plus push / PR events that touch `web/**` or the workflow file itself;
- one job, `web`, with a three-platform matrix (`fail-fast: false`): `windows-latest`
  (`entry: web/server_win.ax`, `out: web/server.exe`, `extra_flags: -l ws2_32`), `ubuntu-latest`
  (`/usr/lib/llvm-18`, `server_posix.ax` → `web/server`) and `macos-14` (`/opt/homebrew/opt/llvm@18`, same as
  Ubuntu); env is `AOXN_LLVM_DIR` plus `NEXT_TELEMETRY_DISABLED=1`;
- steps: checkout → cache cargo (key prefix `-web-cargo-`, with `runner.arch`) → install LLVM per OS →
  `cargo build --release` → `cargo run --release -- build <entry> -o <out> <extra_flags>` →
  `actions/setup-node@v4` (Node 22) → `node web/loadtest/parity.mjs <out>` (functional parity:
  bodies match the Node
  reference, 404 handling, keep-alive, `/metrics`) → inside `web/next-app` run `corepack enable`,
  `pnpm install --frozen-lockfile`, `pnpm build` → download oha v1.16.0 per OS →
  `BENCH_TRIALS=1 BENCH_DURATION=5s node web/loadtest/bench.mjs` (a short benchmark; functional parity is the
  separate `parity.mjs` step before it) → upload `web/loadtest/last-results.json` as artifact `web-bench-<os>`;
- **the numbers produced on CI runners are trend-only** (the file header notes that CI runners share CPUs, so the
  values are not absolutes); see [Web Platform](Web-Platform.md) for the details.

### Local vs CI differences

| Item | Local (dev machine) | CI |
|---|---|---|
| LLVM version | 23.1.0 (winget install) | Windows job installs the latest via winget; the three Tier 2 jobs pin LLVM 18 (`llvm-18-dev` / `llvm@18`) |
| Fixed point `selfhost_driver_self_compiles` | really runs (Windows; byte-for-byte IR and object comparison) | really runs on the Windows job; **self-skips on the Linux/macOS jobs** (it requires `C:\Program Files\LLVM\lib\LLVM-C.lib` and prints a `skipping: standard LLVM install not found` message when absent, then returns) |
| Build cache | local `target/` plus `target/cache` | `actions/cache` caches `~/.cargo/*` and `target`, invalidated by the `Cargo.lock` hash |
| Smoke coverage | run by hand | run by every job; only Windows and Linux assert primes = 9592 |

So "green locally" and "green in CI" are not the same statement: off Windows the byte-exact
fixed-point verification
does not execute (the remaining selfhost tests do). That boundary is also recorded in
[Platform Support](Platform-Support.md).

### Troubleshooting: what to look at when a test fails

1. **Look at the stage first**: `AOXN_TIME=1` prints wall-clock timings for lex / parse / typecheck
   / codegen / link
   plus the codegen sub-phases (`cg.build`, `cg.verify`, `cg.target`, `cg.passes`, `cg.isel`). The stage that fails
   basically names the module to open.
2. **Look at the IR**: `AOXN_DUMP_IR=1` dumps pre-verify IR to stderr; for optimized IR use
   `cargo run -- ir file.ax`. IR-level differences are also what makes the fixed-point test report
   "stage-1 and stage-2 IR differ".
3. **Want machine-readable diagnostics**: `cargo run -- run bad.ax --json` emits
   `{"ok":false,"errors":[{"stage","file","line","col","message"}]}`, where the stage is one of
   `lex | parse | type | internal | link | io`.
4. **"The compiler artifact is missing" is usually a platform extension problem**: off Windows executables have no
   extension and object files are `.o` rather than `.obj`. Tests should use the `EXE` constant and
   `platform::exe_ext()` / `platform::obj_ext()`; self-hosted demos use `exe_suffix()` from `driver.ax`.
5. **A selfhost test cannot find LLVM**: set `AOXN_LLVM_DIR`; on Linux `cannot find -lLLVM-C` is expected, and the
   correct fix is to go through `platform::llvm_link_name()` (or override with `AOXN_LLVM_LIB` in special cases).
6. **`Cannot choose between targets`**: LLVM target registration is process-global, and registering
   the same backend
   twice produces that error — keep `init_target()`'s `Once` semantics.
7. **`unexpected character` at 1:1**: an edited `.ax` fixture picked up a UTF-8 BOM (PowerShell 5.1's
   `-Encoding UTF8` is the usual source); strip the first three bytes.

---

## 源文件 / Source files

- [tests/pipeline.rs](../tests/pipeline.rs) — the suite: helpers (`stdlib_src`, `build_and_run_with_stdlib`,
  `dedent`, `build_and_run`, `expect_compile_error`, `build_and_run_lvl`, `tmp_dir`, `EXE`, `llvm_link_name`,
  `path_with_llvm_bin`, `llvm_dir`), all 97 `#[test]`s, the fixed-point self-skip condition.
- [.github/workflows/ci.yml](../.github/workflows/ci.yml) — the four jobs, cache keys, LLVM install steps, smoke
  assertions.
- [.github/workflows/web-bench.yml](../.github/workflows/web-bench.yml) — the three-platform web matrix, parity and
  benchmark steps, artifact upload.
- [CONTRIBUTING.md](../CONTRIBUTING.md) — test rules, platform-helper preference, ASCII fixtures, local vs CI LLVM
  versions, `--O1` guidance.
- [src/platform.rs](../src/platform.rs) — `llvm_link_name`, `exe_ext`, `obj_ext` (what the tests delegate to).
- [examples/hello.ax](../examples/hello.ax), [examples/strings.ax](../examples/strings.ax),
  [examples/primes.ax](../examples/primes.ax) — the smoke-test programs (primes prints the count below 100000).
- [docs/spec.md](../docs/spec.md) — diagnostics stages, `--json` shape, the tooling contract.
- [Cargo.toml](../Cargo.toml) — dev profile `opt-level = 1` (why a dev-profile compiler is usable), release LTO.
- [CHANGELOG.md](../CHANGELOG.md) — 0.26.2 (suite 93 → 97 tests, the new tests named) and 0.26.3
  verification notes.
- `AGENTS.md` — 本地会话笔记（被 gitignore，不随仓库发布，故此处不设链接）：用于核对固定点跳过条件、
  LLVM 路径与 Windows 工具链事实。
