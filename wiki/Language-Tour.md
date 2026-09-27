# 语言速览 · Language Tour

> **中文**：用一组能直接跑的小程序走完 Aoxn 的全部语法：绑定、控制流、数组与结构体的值语义、字符串与 f-string、泛型、标准库与 C 互操作。
> **English**: A tour of the whole Aoxn syntax through small runnable programs: bindings, control flow, value semantics for arrays and structs, strings and f-strings, generics, the stdlib, and C interop.

## 中文

本页每个片段都是完整程序，可以直接 `cargo run -- run tour.ax`。规则的确切定义在 [语言参考](Language-Reference.md)。

### 1. 你好，Aoxn

```aoxn
def main() -> int:
    print("hello, Aoxn")
    return 0
```

`main` 的返回值就是进程退出码（写 `-> void` 则退出码为 0）。`print` 接受
`int` / `float` / `bool` / `string` 四种类型之一，输出后自动换行；`float` 按
`%f` 打印（6 位小数），`bool` 打印小写的 `true` / `false`。

### 2. 绑定与类型

```aoxn
def main() -> int:
    x = 5                  # 由初值推断：int
    x = x + 1              # 重赋值必须保持同一类型
    y: float = 2.5         # 显式标注（标注必须与初值一致）
    name = "Aoxn"          # string
    flag = True            # bool
    print(f"{x} {y} {name} {flag}")
    return 0
```

输出：

```text
6 2.500000 Aoxn true
```

- 首次赋值即声明，类型固定；再赋别的类型是编译错误。
- **没有隐式转换**：`x + 1.0` 不会自动把 `x` 变成 float，直接报错。
- 同一个函数内不允许遮蔽（shadowing），参数也算已声明的名字。

### 3. 控制流

```aoxn
def classify(n: int) -> string:
    if n < 0:
        return "negative"
    elif n == 0:
        return "zero"
    else:
        return "positive"

def main() -> int:
    print(classify(-3))
    i = 0
    while i < 3:
        print(f"while {i}")
        i = i + 1
    for k in range(3):            # 0, 1, 2
        if k == 1:
            continue              # 跳过本次
        print(f"range {k}")
    for k in range(10, 0, -3):    # 10, 7, 4, 1（step 可以是负数）
        print(f"step {k}")
    return 0
```

要点：条件**必须**是 `bool`；`range` 只能出现在 `for ... in range(...)` 里（它不是普通函数），参数 1–3 个 `int`，起止与步长在进入循环时求值一次；`for x in arr` 遍历数组元素（每个元素是拷贝）。

### 4. 数组

```aoxn
def main() -> int:
    a = [10, 20, 30]                    # 推断为 [int; 3]
    xs: [int; 4] = [1, 2, 3, 4]         # 显式标注
    m: [[int; 2]; 2] = [[1, 2], [3, 4]] # 嵌套数组
    grid: [int; 6] = [0] * 6            # Python 式复制（运行时填充）
    a[1] = 99                           # 元素赋值
    print(len(a))                       # 3，编译期常量
    print(f"{a[0]} {a[1]} {m[1][0]} {grid[5]}")
    b = a                               # 值语义：整份拷贝
    b[0] = -1                           # 不会影响 a
    print(f"{a[0]} {b[0]}")
    return 0
```

输出：`3` / `10 99 3 0` / `10 -1`。

- 数组字面量不能为空（元素类型无法推断），所有元素必须同型。
- 下标是 `int` 且**不做边界检查**：越界是未定义行为（和 C 一样），换来的是零开销。
- 赋值、传参、返回都是**整份拷贝**（编译成 `memcpy`），没有引用/指针。

### 5. 结构体

```aoxn
struct Point:
    x: int
    y: int

struct Segment:
    a: Point
    b: Point

def length_sq(s: Segment) -> int:
    dx = s.b.x - s.a.x
    dy = s.b.y - s.a.y
    return dx * dx + dy * dy

def main() -> int:
    p = Point(x=1, y=2)          # 必须按名给出全部字段，顺序任意
    p.x = 10                     # 字段赋值
    seg = Segment(a=p, b=Point(x=4, y=6))
    print(f"{p.x} {p.y} {length_sq(seg)}")
    ps: [Point; 3] = [Point(x=0, y=0)] * 3   # 结构体也能复制填充
    ps[1].y = 7
    print(f"{ps[0].y} {ps[1].y}")
    return 0
```

输出：`10 2 25` / `0 7`。

- 字段必须带类型标注；结构体可以前向引用（先用后定义）、可以嵌套、可以含数组。
- 结构体**不能包含自己**（直接或间接），否则编译报错。
- 结构体和函数共用一个名字空间；构造语法和函数调用长得很像，但结构体构造要求全部字段按名给出。

### 6. 字符串与 f-string

```aoxn
def main() -> int:
    s = "hello" + ", " + "world"     # + 是拼接（malloc + memcpy）
    print(s)                          # hello, world
    print(len(s))                     # 12（字节数）
    print("apple" < "banana")         # true，逐字节字典序
    print("abc" == "abc")             # true
    n = 3
    print(f"{n} squared is {n * n}")  # f-string 里可放任意表达式
    print(f"braces: {{literal}}")     # {{ }} 是字面花括号
    return 0
```

- 字符串**不可变**，适合放进结构体字段和数组（拷贝只共享底层字节）。
- 拼接结果在堆上分配并且**永不释放**（目前没有 GC）；所以长跑程序不要靠拼接累积数据——需要可变缓冲时用标准库的字节缓冲或 `Vec`。
- 目前没有字符串下标/遍历（没有 `char` 类型），要处理字节用标准库的 `str_get(s, i)`。
- f-string 脱糖成 `"字面量" + str(expr) + ...`；`str()` 支持 `int` / `float` / `bool` / `string`。

### 7. 函数与递归

```aoxn
def fib(n: int) -> int:
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)

def is_even(n: int) -> bool:
    if n == 0:
        return True
    return is_odd(n - 1)

def is_odd(n: int) -> bool:
    if n == 0:
        return False
    return is_even(n - 1)

def main() -> int:
    print(fib(10))
    print(f"{is_even(10)} {is_odd(10)}")
    return 0
```

输出：`55` / `true false`。参数必须带类型标注，返回标注可省略（默认 `void`）；定义顺序无关紧要，递归与相互递归都支持。

### 8. 泛型

```aoxn
import "../stdlib/stdlib.ax"

def first[T, N](arr: [T; N]) -> T:
    return arr[0]

def main() -> int:
    print(first([7, 8, 9]))          # T = int,   N = 3
    print(first(["b", "a"]))         # T = string, N = 2
    s = sort([5, 3, 8, 1])           # stdlib 的泛型排序
    print(f"{s[0]} {s[3]}")
    return 0
```

输出：`7` / `b` / `1 8`。

- `[T, N]` 声明类型参数与数组长度参数；长度参数在函数体里是编译期 `int` 常量（`range(N)`、`N - 1`）。
- 调用点按实参**推断**类型和长度，不做类型标注；每个不同的 `(T, N)` 组合在编译期生成一份专门的实例（单态化），运行时零开销、无装箱。
- 每个实例单独检查：`sort` 要求 `T` 上的 `>`，`int` / `float` / `string` 都行，结构体不行。
- 泛型函数不能是 `main`，也不能是 `extern`；一个函数最多一个长度参数。

### 9. 标准库与 import

```aoxn
import "../stdlib/stdlib.ax"

def main() -> int:
    v = vec_new()
    v = vec_push(v, 42)              # write-back 风格：把结果赋回去
    v = vec_push(v, 7)
    print(f"{vec_get(v, 0)} {vec_get(v, 1)} len={v.len}")
    print(f"gcd={gcd(24, 18)} isqrt={isqrt(1000)} prime={is_prime(97)}")
    ok = write_file("out.txt", "hello")
    print(f"written={ok} read={read_file('out.txt')}")
    return 0
```

输出：`42 7 len=2` / `gcd=6 isqrt=31 prime=true` / `written=true read=hello`。

`import` 把另一个 `.ax` 文件并入**同一个名字空间**（相对导入者路径解析，每个文件只包含一次，循环导入报错）。标准库的完整清单见 [标准库](Standard-Library.md)。

### 10. 与 C 互操作

```aoxn
extern def sqrt(x: float) -> float

def main() -> int:
    print(sqrt(2.0))                 # 1.414214（%f，6 位小数）
    return 0
```

`extern def` 声明一个没有函数体的 C 函数，链接时从默认库解析。需要额外库时用 `-l` / `-L` 透传给 clang：

```powershell
cargo run -- run web\server_win.ax -l ws2_32
```

指针以 `int` 承载、字符串就是 NUL 结尾的字节缓冲，这是标准库与自举编译器调用操作系统的通道（`malloc`/`realloc`/`fopen`/`system` 都这么声明）。

### 11. 会被拒绝的写法（严格性是特性）

```aoxn
def bad1(n: int) -> int:
    if n > 0:
        return 1                 # 错误：并非所有路径都 return
```

```text
error[type] 2:5: function 'bad1' returns int but does not return a value on all paths
```

| 写法 | 结果 |
|---|---|
| `x = 1 + 1.5` | 错误：`int` 与 `float` 不能隐式混合 |
| `if 1:` | 错误：条件必须是 `bool` |
| `x = 1` 然后 `x = "s"` | 错误：重赋值不能改变类型 |
| `print(p)`（`p` 是结构体） | 错误：`print` 只接受标量四种类型 |
| `arr1 == arr2`（聚合比较） | 错误：聚合不能用 `==` |
| `return 1` 之后再写语句 | 错误：不可达代码 |
| `def f[N](x: int) -> int:` 里用 `N` 却不是数组长度 | 错误：长度参数必须用于 `[T; N]` |
| 顶层直接写 `print("hi")` | 错误：顶层只允许 `import` / `struct` / `def` / `extern def` |

### 12. 下一步

- 完整规则与边界：[语言参考](Language-Reference.md)
- 标准库清单：[标准库](Standard-Library.md)
- 命令行与构建缓存：[命令行与工具链](CLI-and-Tooling.md)
- 报错看不懂：[常见问题与排错](Troubleshooting-FAQ.md)

## English

Every snippet below is a complete program you can run with
`cargo run -- run tour.ax`. Exact rules live in the
[Language Reference](Language-Reference.md).

### 1. Hello, Aoxn

```aoxn
def main() -> int:
    print("hello, Aoxn")
    return 0
```

`main`'s return value is the process exit code (declare `-> void` and it exits
with 0). `print` accepts exactly one `int` / `float` / `bool` / `string`,
appends a newline, prints `float` with `%f` (6 decimals) and `bool` as lowercase
`true` / `false`.

### 2. Bindings and types

```aoxn
def main() -> int:
    x = 5                  # inferred: int
    x = x + 1              # re-assignment keeps the type
    y: float = 2.5         # explicit annotation (must match the initializer)
    name = "Aoxn"          # string
    flag = True            # bool
    print(f"{x} {y} {name} {flag}")
    return 0
```

Output:

```text
6 2.500000 Aoxn true
```

- The first assignment declares the name and fixes its type; assigning another
  type later is a compile error.
- **No implicit conversions**: `x + 1.0` does not promote `x` to float, it is an
  error.
- Shadowing is not allowed inside a function; parameters count as declared names.

### 3. Control flow

```aoxn
def classify(n: int) -> string:
    if n < 0:
        return "negative"
    elif n == 0:
        return "zero"
    else:
        return "positive"

def main() -> int:
    print(classify(-3))
    i = 0
    while i < 3:
        print(f"while {i}")
        i = i + 1
    for k in range(3):            # 0, 1, 2
        if k == 1:
            continue              # skip this iteration
        print(f"range {k}")
    for k in range(10, 0, -3):    # 10, 7, 4, 1 (step may be negative)
        print(f"step {k}")
    return 0
```

Conditions **must** be `bool`. `range` exists only inside `for ... in range(...)`
(it is not an ordinary function), takes 1–3 `int` arguments, and its bounds and
step are evaluated once at loop entry. `for x in arr` iterates array elements,
each copied into the loop variable.

### 4. Arrays

```aoxn
def main() -> int:
    a = [10, 20, 30]                    # inferred [int; 3]
    xs: [int; 4] = [1, 2, 3, 4]         # annotated
    m: [[int; 2]; 2] = [[1, 2], [3, 4]] # nested
    grid: [int; 6] = [0] * 6            # Python-style replication (runtime fill)
    a[1] = 99                           # element assignment
    print(len(a))                       # 3, a compile-time constant
    print(f"{a[0]} {a[1]} {m[1][0]} {grid[5]}")
    b = a                               # value semantics: a full copy
    b[0] = -1                           # does not touch a
    print(f"{a[0]} {b[0]}")
    return 0
```

Output: `3` / `10 99 3 0` / `10 -1`.

- Array literals cannot be empty (the element type could not be inferred) and all
  elements must share one type.
- The index is an `int` and **unchecked**: out-of-bounds is undefined behaviour
  (like C), which is what buys the zero overhead.
- Assignment, parameter passing and returns **copy the whole value** (lowered to
  `memcpy`); there are no references or pointers.

### 5. Structs

```aoxn
struct Point:
    x: int
    y: int

struct Segment:
    a: Point
    b: Point

def length_sq(s: Segment) -> int:
    dx = s.b.x - s.a.x
    dy = s.b.y - s.a.y
    return dx * dx + dy * dy

def main() -> int:
    p = Point(x=1, y=2)          # every field, by name, in any order
    p.x = 10                     # field assignment
    seg = Segment(a=p, b=Point(x=4, y=6))
    print(f"{p.x} {p.y} {length_sq(seg)}")
    ps: [Point; 3] = [Point(x=0, y=0)] * 3   # structs replicate too
    ps[1].y = 7
    print(f"{ps[0].y} {ps[1].y}")
    return 0
```

Output: `10 2 25` / `0 7`.

- Fields need type annotations; structs may forward-reference each other, nest,
  and contain arrays.
- A struct **cannot contain itself**, directly or indirectly.
- Structs and functions share one namespace. Construction looks like a call but
  requires every field by name.

### 6. Strings and f-strings

```aoxn
def main() -> int:
    s = "hello" + ", " + "world"     # + concatenates (malloc + memcpy)
    print(s)                          # hello, world
    print(len(s))                     # 12 (bytes)
    print("apple" < "banana")         # true, byte-wise lexicographic
    print("abc" == "abc")             # true
    n = 3
    print(f"{n} squared is {n * n}")  # any expression inside {}
    print(f"braces: {{literal}}")     # {{ }} is a literal brace
    return 0
```

- Strings are **immutable**, which is why they can live in struct fields and
  arrays (copies share the underlying bytes).
- Concatenation allocates on the heap and is **never freed** (there is no GC
  yet), so a long-running program must not accumulate data by concatenation —
  use the stdlib byte buffers or `Vec` instead.
- There is no string indexing or iteration (no `char` type yet); use the stdlib
  `str_get(s, i)` for bytes.
- f-strings desugar to `"lit" + str(expr) + ...`; `str()` handles
  `int` / `float` / `bool` / `string`.

### 7. Functions and recursion

```aoxn
def fib(n: int) -> int:
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)

def is_even(n: int) -> bool:
    if n == 0:
        return True
    return is_odd(n - 1)

def is_odd(n: int) -> bool:
    if n == 0:
        return False
    return is_even(n - 1)

def main() -> int:
    print(fib(10))
    print(f"{is_even(10)} {is_odd(10)}")
    return 0
```

Output: `55` / `true false`. Parameters require type annotations, the return
annotation is optional (defaults to `void`), definition order does not matter,
and recursion plus mutual recursion are supported.

### 8. Generics

```aoxn
import "../stdlib/stdlib.ax"

def first[T, N](arr: [T; N]) -> T:
    return arr[0]

def main() -> int:
    print(first([7, 8, 9]))          # T = int,    N = 3
    print(first(["b", "a"]))         # T = string, N = 2
    s = sort([5, 3, 8, 1])           # the stdlib's generic sort
    print(f"{s[0]} {s[3]}")
    return 0
```

Output: `7` / `b` / `1 8`.

- `[T, N]` declares type parameters and one array-length parameter; inside the
  body the length parameter is a compile-time `int` constant (`range(N)`,
  `N - 1`).
- Call sites **infer** the types and length (no explicit type arguments) and each
  distinct `(T, N)` combination gets its own compile-time instance
  (monomorphization): no boxing, no runtime dispatch.
- Every instance is checked separately: `sort` needs `>` on `T`, which works for
  `int` / `float` / `string` but not for structs.
- A generic function cannot be `main` and cannot be `extern`; at most one length
  parameter per function.

### 9. The standard library and imports

```aoxn
import "../stdlib/stdlib.ax"

def main() -> int:
    v = vec_new()
    v = vec_push(v, 42)              # write-back style: assign the result back
    v = vec_push(v, 7)
    print(f"{vec_get(v, 0)} {vec_get(v, 1)} len={v.len}")
    print(f"gcd={gcd(24, 18)} isqrt={isqrt(1000)} prime={is_prime(97)}")
    ok = write_file("out.txt", "hello")
    print(f"written={ok} read={read_file('out.txt')}")
    return 0
```

Output: `42 7 len=2` / `gcd=6 isqrt=31 prime=true` / `written=true read=hello`.

`import` merges another `.ax` file into **one namespace** (paths resolve relative
to the importing file, each file is included once, import cycles are errors).
The full inventory is in the [Standard Library](Standard-Library.md).

### 10. C interop

```aoxn
extern def sqrt(x: float) -> float

def main() -> int:
    print(sqrt(2.0))                 # 1.414214 (%f, 6 decimals)
    return 0
```

`extern def` declares a C function with no body, resolved at link time from the
default libraries. Extra libraries are forwarded to clang with `-l` / `-L`:

```powershell
cargo run -- run web\server_win.ax -l ws2_32
```

Pointers travel as plain `int` and strings are NUL-terminated byte buffers —
this is the channel the stdlib and the self-hosted compiler use to reach the
operating system (`malloc` / `realloc` / `fopen` / `system` are all declared
this way).

### 11. What gets rejected (strictness is the feature)

```aoxn
def bad1(n: int) -> int:
    if n > 0:
        return 1                 # error: not all paths return
```

```text
error[type] 2:5: function 'bad1' returns int but does not return a value on all paths
```

| Construct | Result |
|---|---|
| `x = 1 + 1.5` | error: `int` and `float` never mix implicitly |
| `if 1:` | error: the condition must be `bool` |
| `x = 1` then `x = "s"` | error: re-assignment cannot change the type |
| `print(p)` where `p` is a struct | error: `print` takes one of the four scalar types |
| `arr1 == arr2` | error: aggregates cannot be compared |
| a statement after `return 1` | error: unreachable code |
| `def f[N](x: int) -> int:` using `N` without an array length | error: the length parameter must be used in `[T; N]` |
| a top-level `print("hi")` | error: top level allows only `import` / `struct` / `def` / `extern def` |

### 12. Next steps

- Full rules and edge cases: [Language Reference](Language-Reference.md)
- What ships in the stdlib: [Standard Library](Standard-Library.md)
- CLI and build cache: [CLI and Tooling](CLI-and-Tooling.md)
- When an error makes no sense: [Troubleshooting and FAQ](Troubleshooting-FAQ.md)

---

## 源文件 / Source files

- [docs/spec.md](../docs/spec.md) — the language specification (labelled v0.9; some sections are outdated)
- [src/parser.rs](../src/parser.rs) — the grammar as implemented (precedence, layout, generics headers)
- [src/typecheck.rs](../src/typecheck.rs) — the strict rules and the builtins
- [stdlib/stdlib.ax](../stdlib/stdlib.ax) — generic `sort` / `binary_search` / `Vec` / file IO
- [examples/hello.ax](../examples/hello.ax), [examples/fib.ax](../examples/fib.ax), [examples/stdlib_demo.ax](../examples/stdlib_demo.ax) — runnable examples
- [tests/pipeline.rs](../tests/pipeline.rs) — executable statements of every rule above
