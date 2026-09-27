# 标准库 · Standard Library

> **中文**：`stdlib/stdlib.ax` 用 Aoxn 自己写成：数学、素数、泛型搜索/排序/聚合、`Vec`、字节缓冲、文件 IO 与进程调用。本页是它的完整 API 参考与使用注意。
> **English**: `stdlib/stdlib.ax` is written in Aoxn itself: math, primality, generic search/sort/aggregation, `Vec`, byte buffers, file IO and process spawning. This page is its complete API reference plus usage notes.

## 中文

### 1. 它是什么，怎么用

标准库**不是**预编译库，也没有包管理：它就是一个 `.ax` 文件，`import` 进来与你的程序一起编译（每个文件只包含一次）。

```aoxn
import "../stdlib/stdlib.ax"

def main() -> int:
    print(sum_int([1, 2, 3]))
    return 0
```

- 路径相对**导入者所在文件**解析，所以要按自己文件的层级写对（仓库根目录下的程序写 `import "stdlib/stdlib.ax"`）。
- 导入后所有名字进入**同一个命名空间**：`sort`、`Vec`、`read_file` 都可能和你的名字冲突，重名会报错。
- 泛型函数（`sort` / `binary_search` / `max_of` …）在每个调用点按 `(T, N)` 单态化，没有运行时开销。

### 2. C 运行时数学（FFI）

| 函数 | 签名 | 说明 |
|---|---|---|
| `sqrt` | `sqrt(x: float) -> float` | C `sqrt` |
| `floor` | `floor(x: float) -> float` | C `floor` |
| `ceil` | `ceil(x: float) -> float` | C `ceil` |

这三个是 `extern def`，由 clang 链到系统 C 库；其余数学函数都用 Aoxn 写。

### 3. 整数数学

| 函数 | 签名 | 说明 |
|---|---|---|
| `abs_i` | `abs_i(n: int) -> int` | 绝对值（`n < 0` 时取负） |
| `min_i` / `max_i` | `(a: int, b: int) -> int` | 两值取小/取大 |
| `clamp_i` | `clamp_i(v: int, lo: int, hi: int) -> int` | 夹到 `[lo, hi]` |
| `pow_i` | `pow_i(base: int, exp: int) -> int` | 快速幂（平方-乘），`exp >= 0`；溢出按 C 语义回绕 |
| `gcd` | `gcd(a: int, b: int) -> int` | 欧几里得算法，先取绝对值 |
| `lcm` | `lcm(a: int, b: int) -> int` | 最小公倍数；任一为 0 时返回 0 |
| `isqrt` | `isqrt(n: int) -> int` | 整数平方根（牛顿法，向下取整） |

```aoxn
print(f"{gcd(24, 18)} {isqrt(1000)} {pow_i(2, 10)}")   # 6 31 1024
```

### 4. 浮点数学

| 函数 | 签名 | 说明 |
|---|---|---|
| `abs_f` | `abs_f(x: float) -> float` | 绝对值 |
| `min_f` / `max_f` | `(a: float, b: float) -> float` | 两值取小/取大 |
| `clamp_f` | `clamp_f(v: float, lo: float, hi: float) -> float` | 夹到 `[lo, hi]` |
| `hypot` | `hypot(a: float, b: float) -> float` | `sqrt(a*a + b*b)` |

### 5. 素数

| 函数 | 签名 | 说明 |
|---|---|---|
| `is_prime` | `is_prime(n: int) -> bool` | 试除到 `sqrt(n)`；`n < 2` 为 `False` |

### 6. 泛型搜索

| 函数 | 签名 | 说明 |
|---|---|---|
| `linear_search` | `linear_search[T, N](arr: [T; N], target: T) -> int` | 首个匹配的下标，未找到返回 `-1` |
| `binary_search` | `binary_search[T, N](arr: [T; N], target: T) -> int` | 二分查找，**要求数组升序**；未找到返回 `-1` |

`linear_search` 只需要 `T` 上的 `==`；`binary_search` 还需要 `<`。两者对 `int` / `float` / `string` 都可用，结构体不可用。

### 7. 泛型排序与聚合

| 函数 | 签名 | 说明 |
|---|---|---|
| `sort` | `sort[T, N](arr: [T; N]) -> [T; N]` | 升序冒泡排序，**返回排好序的副本，输入不变** |
| `reverse` | `reverse[T, N](arr: [T; N]) -> [T; N]` | 返回反转后的副本 |
| `max_of` / `min_of` | `max_of[T, N](arr: [T; N]) -> T` | 最大值 / 最小值（要求 `T` 上可比） |
| `sum_int` | `sum_int[N](arr: [int; N]) -> int` | `int` 数组求和 |
| `sum_float` | `sum_float[N](arr: [float; N]) -> float` | `float` 数组求和 |

```aoxn
import "../stdlib/stdlib.ax"

def main() -> int:
    nums = [5, 3, 8, 1]
    s = sort(nums)                       # nums 本身不变（值语义）
    print(f"{s[0]} {s[3]} {nums[0]}")    # 1 8 5
    print(f"{binary_search(s, 5)} {linear_search(s, 99)}")   # 2 -1
    return 0
```

### 8. `Vec`：可增长的 8 字节槽数组

```aoxn
struct Vec:
    data: int      # 指向 malloc 得到的缓冲区（空 Vec 时是 0）
    len: int       # 已用槽数
    cap: int       # 已分配槽数
```

| 函数 | 签名 | 说明 |
|---|---|---|
| `vec_new` | `vec_new() -> Vec` | 空 Vec（`data=0, len=0, cap=0`） |
| `vec_push` | `vec_push(v: Vec, item: int) -> Vec` | 追加一个槽；容量不足时 `realloc` 到 2 倍（首次 8 槽） |
| `vec_get` | `vec_get(v: Vec, i: int) -> int` | 读第 `i` 个槽（**不检查边界**） |
| `vec_set` | `vec_set(v: Vec, i: int, item: int)` | 写第 `i` 个槽（无返回值） |
| `vec_pop` | `vec_pop(v: Vec) -> Vec` | 返回 `len - 1` 的新 Vec（不缩容、不清零、不释放） |
| `vec_free` | `vec_free(v: Vec)` | `free(data)` |

**唯一的槽宽是 8 字节**：可以放 `int`、`bool`（0/1），字符串要先用 `as_ptr(s)` 转成地址再存。

**写回风格是必须的**：`Vec` 是值语义结构体，`vec_push` 收到的是拷贝，所以一定要把结果赋回去：

```aoxn
v = vec_new()
v = vec_push(v, 42)          # 正确
# vec_push(v, 42)            # 错误：改的是拷贝，v 不变
i = 0
while i < v.len:
    print(vec_get(v, i))
    i = i + 1
vec_free(v)
```

### 9. 字节与字符串缓冲

| 函数 | 签名 | 说明 |
|---|---|---|
| `fill_zero` | `fill_zero(p: int, n: int)` | 把 `n` 个字节写成 0 |
| `buf_new` | `buf_new(cap: int) -> int` | `malloc(cap)` + 清零，返回地址 |
| `str_get` | `str_get(s: string, i: int) -> int` | 取字符串第 `i` 个**字节**（0–255） |
| `is_digit` | `is_digit(c: int) -> bool` | `48..57` |
| `is_alpha` | `is_alpha(c: int) -> bool` | `A-Z` 或 `a-z` |
| `is_space` | `is_space(c: int) -> bool` | 空格 / `\t` / `\n` / `\r` |

字符串本身不可变、不能索引，所以"就地构建文本"要用 `buf_new` + `store_u8(addr, off, byte)`，
再从地址 `as_string(addr)` 读出来（`web/http_buf.ax` 就是这么渲染 HTTP 响应的）。

### 10. 文件 IO

| 函数 | 签名 | 说明 |
|---|---|---|
| `read_file` | `read_file(path: string) -> string` | 读整个文件；打不开返回空字符串 |
| `write_file` | `write_file(path: string, content: string) -> bool` | 写整个字符串；打不开返回 `False` |
| `fopen` / `fread` / `fwrite` / `fseek` / `ftell` / `fclose` | 见 `extern def` 声明 | 底层 C 接口，需要自己管缓冲区时用 |

```aoxn
ok = write_file("notes.txt", "hello")
print(f"{ok} {read_file("notes.txt")}")     # true hello
```

注意：`read_file` 用"空字符串"表示失败，所以**无法区分"空文件"与"打开失败"**；
需要区分就直接用 `fopen` 并检查返回的句柄是否为 `0`。

### 11. 进程调用

| 函数 | 签名 | 说明 |
|---|---|---|
| `system` | `system(cmd: string) -> int` | 直接暴露 C `system()`：POSIX 返回 **wait status**，Windows 返回退出码 |
| `system_exit_code` | `system_exit_code(cmd: string) -> int` | 归一化后的退出码，跨平台一致 |

```aoxn
code = system_exit_code("exit 0")
print(code)                                  # 0
print(system_exit_code("exit 7"))            # 7（Windows 与 POSIX 上归一化结果一致）
```

注意 `system` 走的是 **shell 的 PATH**：如果 clang 不在 PATH 上，`system("clang --version")`
会返回非 0——这与编译器自己查找 clang 的路径顺序（`AOXN_CLANG` → PATH → 默认安装位置）无关。

`system_exit_code` 的规则：Windows 直接返回；POSIX 上把 wait status 折算成退出码，被信号杀死按 shell 惯例报 `128 + 信号`，shell 起不来返回 `-1`。**写跨平台代码时永远用它**。

### 12. 已知边界与坑

| 事项 | 说明 |
|---|---|
| 内存不会回收 | 字符串拼接、`read_file`、`buf_new`、`Vec` 的缓冲区都需要手动 `free`（或干脆不 free）；语言没有 GC |
| 空 `Vec` 越界 | `vec_new()` 的 `data` 是 0：`vec_get(v, -1)` 会访问 `-8` 这类非法地址（`vec_get(v, 0)` 才是解引用 NULL），两者都直接崩 |
| `vec_pop` 不缩容 | 只是返回更短的 Vec，内存仍在（要对齐 C 语义就自己 `realloc`） |
| 槽不是字节 | `vec_get(i)` 取第 `i` 个 8 字节槽；字节级操作请用 `load_u8` / `store_u8` / `str_get` |
| 没有 `char` 类型 | `is_digit` 之类接受的是字节值 `int`，不是字符 |
| 极值 | `abs_i(INT_MIN)` 会溢出（有符号溢出是 UB）；`pow_i` 溢出按 C 回绕 |
| 非 ASCII 路径 | 自举编译器里的文件读取用窄字符 `fopen`，非 ASCII 路径会失败（Rust 编译器不受影响；测试 fixture 保持 ASCII） |
| 命名空间 | 标准库没有前缀，所有名字平铺进你的程序命名空间 |

### 13. 想扩展标准库

直接编辑 `stdlib/stdlib.ax` 加函数即可（纯 Aoxn，不需要动 Rust）：

1. 泛型算法用 `def f[T, N](arr: [T; N])` 形式，长度参数最多一个；
2. `Vec` / 缓冲区这类可变容器遵循**写回风格**（返回新结构体，让调用者赋值）；
3. 加完跑 `cargo test`——`tests/pipeline.rs` 里有 stdlib 相关用例，自举侧的
   `selfhost_frontend_handles_stdlib` 会检查整个标准库能否通过 Aoxn 写的类型检查器；
4. 改标准库会进入自举固定点对比（IR 与 COFF 逐字节一致），所以两边都要保持可编译。

## English

### 1. What it is and how to use it

The standard library is **not** a prebuilt library and there is no package
manager: it is one `.ax` file that you `import` and compile together with your
program (each file is included once).

```aoxn
import "../stdlib/stdlib.ax"

def main() -> int:
    print(sum_int([1, 2, 3]))
    return 0
```

- The path resolves relative to **the importing file**, so mind your directory
  depth (a program at the repository root writes
  `import "stdlib/stdlib.ax"`).
- Imported names land in **one namespace**: `sort`, `Vec` and `read_file` can
  collide with your own names, and a collision is an error.
- Generic functions (`sort`, `binary_search`, `max_of`, …) are monomorphized per
  `(T, N)` at each call site — no runtime overhead.

### 2. C runtime math (FFI)

| Function | Signature | Notes |
|---|---|---|
| `sqrt` | `sqrt(x: float) -> float` | C `sqrt` |
| `floor` | `floor(x: float) -> float` | C `floor` |
| `ceil` | `ceil(x: float) -> float` | C `ceil` |

These three are `extern def`s linked against the system C library by clang;
everything else in the stdlib is written in Aoxn.

### 3. Integer math

| Function | Signature | Notes |
|---|---|---|
| `abs_i` | `abs_i(n: int) -> int` | absolute value (negates when `n < 0`) |
| `min_i` / `max_i` | `(a: int, b: int) -> int` | smaller / larger |
| `clamp_i` | `clamp_i(v: int, lo: int, hi: int) -> int` | clamp into `[lo, hi]` |
| `pow_i` | `pow_i(base: int, exp: int) -> int` | fast exponentiation (square-and-multiply), `exp >= 0`; overflow wraps like C |
| `gcd` | `gcd(a: int, b: int) -> int` | Euclid, absolute values first |
| `lcm` | `lcm(a: int, b: int) -> int` | least common multiple; `0` when either is `0` |
| `isqrt` | `isqrt(n: int) -> int` | integer square root (Newton's method, floors) |

```aoxn
print(f"{gcd(24, 18)} {isqrt(1000)} {pow_i(2, 10)}")   # 6 31 1024
```

### 4. Float math

| Function | Signature | Notes |
|---|---|---|
| `abs_f` | `abs_f(x: float) -> float` | absolute value |
| `min_f` / `max_f` | `(a: float, b: float) -> float` | smaller / larger |
| `clamp_f` | `clamp_f(v: float, lo: float, hi: float) -> float` | clamp into `[lo, hi]` |
| `hypot` | `hypot(a: float, b: float) -> float` | `sqrt(a*a + b*b)` |

### 5. Primality

| Function | Signature | Notes |
|---|---|---|
| `is_prime` | `is_prime(n: int) -> bool` | trial division up to `sqrt(n)`; `n < 2` is `False` |

### 6. Generic search

| Function | Signature | Notes |
|---|---|---|
| `linear_search` | `linear_search[T, N](arr: [T; N], target: T) -> int` | index of the first match, `-1` when absent |
| `binary_search` | `binary_search[T, N](arr: [T; N], target: T) -> int` | binary search; **the array must be sorted ascending**; `-1` when absent |

`linear_search` needs `==` on `T`; `binary_search` also needs `<`. Both work for `int` / `float` / `string`, not for structs.

### 7. Generic sort and aggregation

| Function | Signature | Notes |
|---|---|---|
| `sort` | `sort[T, N](arr: [T; N]) -> [T; N]` | ascending bubble sort; **returns a sorted copy, the input is untouched** |
| `reverse` | `reverse[T, N](arr: [T; N]) -> [T; N]` | returns a reversed copy |
| `max_of` / `min_of` | `max_of[T, N](arr: [T; N]) -> T` | maximum / minimum (needs a comparable `T`) |
| `sum_int` | `sum_int[N](arr: [int; N]) -> int` | sum of an `int` array |
| `sum_float` | `sum_float[N](arr: [float; N]) -> float` | sum of a `float` array |

```aoxn
import "../stdlib/stdlib.ax"

def main() -> int:
    nums = [5, 3, 8, 1]
    s = sort(nums)                       # nums itself is unchanged (value semantics)
    print(f"{s[0]} {s[3]} {nums[0]}")    # 1 8 5
    print(f"{binary_search(s, 5)} {linear_search(s, 99)}")   # 2 -1
    return 0
```

### 8. `Vec`: a growable array of 8-byte slots

```aoxn
struct Vec:
    data: int      # malloc'd buffer (0 for an empty Vec)
    len: int       # used slots
    cap: int       # allocated slots
```

| Function | Signature | Notes |
|---|---|---|
| `vec_new` | `vec_new() -> Vec` | empty Vec (`data=0, len=0, cap=0`) |
| `vec_push` | `vec_push(v: Vec, item: int) -> Vec` | append one slot; `realloc`s to double capacity when full (8 slots first) |
| `vec_get` | `vec_get(v: Vec, i: int) -> int` | read slot `i` (**no bounds check**) |
| `vec_set` | `vec_set(v: Vec, i: int, item: int)` | write slot `i` (no return value) |
| `vec_pop` | `vec_pop(v: Vec) -> Vec` | returns a Vec with `len - 1` (no shrinking, no clearing, no free) |
| `vec_free` | `vec_free(v: Vec)` | `free(data)` |

**The only slot width is 8 bytes**: store `int`, `bool` (0/1), or a string after
converting it with `as_ptr(s)`.

**Write-back style is mandatory**: `Vec` is a value-semantics struct, so
`vec_push` receives a copy — always assign the result back:

```aoxn
v = vec_new()
v = vec_push(v, 42)          # correct
# vec_push(v, 42)            # wrong: it mutates a copy, v is unchanged
i = 0
while i < v.len:
    print(vec_get(v, i))
    i = i + 1
vec_free(v)
```

### 9. Byte and string buffers

| Function | Signature | Notes |
|---|---|---|
| `fill_zero` | `fill_zero(p: int, n: int)` | write `n` zero bytes |
| `buf_new` | `buf_new(cap: int) -> int` | `malloc(cap)` + zero fill, returns the address |
| `str_get` | `str_get(s: string, i: int) -> int` | byte `i` of a string (0–255) |
| `is_digit` | `is_digit(c: int) -> bool` | `48..57` |
| `is_alpha` | `is_alpha(c: int) -> bool` | `A-Z` or `a-z` |
| `is_space` | `is_space(c: int) -> bool` | space / `\t` / `\n` / `\r` |

Strings are immutable and cannot be indexed, so building text in place means
`buf_new` + `store_u8(addr, off, byte)` and reading it back with
`as_string(addr)` — which is exactly how `web/http_buf.ax` renders HTTP
responses.

### 10. File IO

| Function | Signature | Notes |
|---|---|---|
| `read_file` | `read_file(path: string) -> string` | read a whole file; returns an empty string when it cannot be opened |
| `write_file` | `write_file(path: string, content: string) -> bool` | write a whole string; `False` when it cannot be opened |
| `fopen` / `fread` / `fwrite` / `fseek` / `ftell` / `fclose` | see the `extern def`s | the underlying C API, for when you manage buffers yourself |

```aoxn
ok = write_file("notes.txt", "hello")
print(f"{ok} {read_file("notes.txt")}")     # true hello
```

Note: `read_file` signals failure with an empty string, so it **cannot
distinguish an empty file from a failed open**; when that matters, call `fopen`
directly and check whether the handle is `0`.

### 11. Process spawning

| Function | Signature | Notes |
|---|---|---|
| `system` | `system(cmd: string) -> int` | raw C `system()`: POSIX returns a **wait status**, Windows an exit code |
| `system_exit_code` | `system_exit_code(cmd: string) -> int` | normalized exit code, identical across platforms |

```aoxn
code = system_exit_code("exit 0")
print(code)                                  # 0
print(system_exit_code("exit 7"))            # 7 (the same on Windows and POSIX)
```

Note that `system` uses the **shell's PATH**: when clang is not on `PATH`,
`system("clang --version")` returns non-zero — unrelated to the compiler's own
clang lookup order (`AOXN_CLANG` → `PATH` → the default install locations).

`system_exit_code` returns the Windows exit code directly, converts a POSIX wait
status into an exit code (a signal death is reported shell-style as
`128 + signal`), and returns `-1` when the shell cannot start. **Always use it in
cross-platform code.**

### 12. Known limits and traps

| Item | Detail |
|---|---|
| Nothing is reclaimed | string concatenation, `read_file`, `buf_new` and `Vec` buffers need an explicit `free` (or are simply leaked); there is no GC |
| Empty-`Vec` overrun | `vec_new()` has `data == 0`, so `vec_get(v, -1)` touches an invalid address such as `-8` (it is `vec_get(v, 0)` that dereferences NULL); either way it crashes |
| `vec_pop` does not shrink | it only returns a shorter Vec; the memory stays (call `realloc` yourself for C semantics) |
| Slots are not bytes | `vec_get(i)` reads the `i`-th 8-byte slot; for byte work use `load_u8` / `store_u8` / `str_get` |
| No `char` type | `is_digit` and friends take a byte value as `int`, not a character |
| Extremes | `abs_i(INT_MIN)` overflows (signed overflow is UB); `pow_i` wraps like C |
| Non-ASCII paths | the self-hosted compiler's file reader uses narrow `fopen`, so non-ASCII paths fail there (the Rust compiler is unaffected; test fixtures stay ASCII) |
| Namespace | the stdlib has no prefix — every name is flattened into your program's namespace |

### 13. Extending the stdlib

Just edit `stdlib/stdlib.ax` (pure Aoxn, no Rust needed):

1. write generic algorithms as `def f[T, N](arr: [T; N])`, with at most one
   length parameter;
2. mutable containers (`Vec`, buffers) follow **write-back style**: return a new
   struct and let the caller assign it;
3. run `cargo test` afterwards — `tests/pipeline.rs` covers the stdlib, and the
   self-hosted `selfhost_frontend_handles_stdlib` checks that the whole standard
   library typechecks under the Aoxn-written checker;
4. stdlib changes take part in the self-hosting fixed point (byte-identical IR
   and COFF objects), so both compilers must keep compiling it.

---

## 源文件 / Source files

- [stdlib/stdlib.ax](../stdlib/stdlib.ax) — the library itself (every signature above was read from it)
- [tests/pipeline.rs](../tests/pipeline.rs) — `stdlib_*` tests pin the documented behavior
- [examples/stdlib_demo.ax](../examples/stdlib_demo.ax) — a program using the stdlib
- [web/http_buf.ax](../web/http_buf.ax) — real byte-buffer rendering with `store_u8`
- [docs/spec.md](../docs/spec.md) — the raw-memory builtins the stdlib is built on
