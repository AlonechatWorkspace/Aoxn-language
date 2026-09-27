# 语言参考 · Language Reference

> **中文**：Aoxn 语言的完整规则：词法与布局、类型与运算符、语句、数组与结构体的值语义、字符串与 f-string、泛型单态化、导入、C 互操作、内建函数，以及明确不存在的特性。
> **English**: The complete rules of Aoxn: layout and lexical structure, types and operators, statements, value semantics for arrays and structs, strings and f-strings, generic monomorphization, imports, C interop, builtins, and the features that deliberately do not exist.

## 中文

本页以 v0.26.3 的 `src/parser.rs` + `src/typecheck.rs` 实现为准，并给出实测到的诊断文本。
权威规范文本是 [`docs/spec.md`](../docs/spec.md)（标称 v0.9，个别章节已落后，本页以实现为准）。

### 1. 词法与布局

| 规则 | 说明 |
|---|---|
| 块结构 | 由**缩进**定义，不用花括号。宽度任意但要一致；Tab 按 4 列计算 |
| 语句结束 | 行尾即语句结束；**没有分号**（`;` 只出现在 `[T; N]` 类型里） |
| 注释 | `#` 到行尾。空行与纯注释行不产生任何 token，也不影响缩进 |
| 续行 | 括号 `(...)` 内的换行被忽略（隐式续行）；其他地方换行即语句结束 |
| 单行块 | `if n < 2: return n` 合法（`:` 后只允许**简单语句**） |
| 复合语句 | 不能写在 `:` 同一行（`if` / `while` / `for` / `def` / `struct` 会报 `compound statements cannot appear on the same line after ':'`） |
| f-string | `f"..."` 前缀，`{expr}` 插值，`{{` / `}}` 是字面花括号 |
| 字符串字面量 | 只有**双引号**（没有单引号、没有原始字符串） |
| 转义 | `\n` `\t` `\\` `\"`（**没有** `\r`、没有十六进制/Unicode 转义） |
| 数字字面量 | 十进制整数与浮点（浮点必须带小数点）；**没有十六进制/二进制字面量** |
| 注释风格提醒 | `//` 既不是注释也不是运算符：`5 // 2` 是 `expected an expression, found Slash` |

缩进不匹配的报错是 `unindent does not match any outer indentation level`。

### 2. 类型

| 类型 | 含义 | 后端表示 |
|---|---|---|
| `int` | 有符号 64 位整数 | i64 |
| `float` | 64 位 IEEE 浮点 | double |
| `bool` | `True` / `False`（也接受 `true` / `false`） | i1 |
| `string` | 不可变字节序列，NUL 结尾 | 指向字节的指针 |
| `void` | 无值（只用于返回类型） | 无 |
| `[T; N]` | 定长数组，`N > 0` 的整数字面量 | `[N x T]` |
| `Name` | 结构体 | 命名的 struct |

语义上的"不安全但快"（与 C 一致，属于**有意的设计**）：

- `int` 与 `float` **绝不隐式转换**（连字面量也不转换）。
- 有符号整数溢出是未定义行为（发射时带 `nsw`）。
- 除以零是未定义行为（不插入运行时检查）。
- 数组下标**不做边界检查**（C 风格）。
- 原始内存内建（第 13 节）完全不检查。

### 3. 程序结构

顶层只允许四种声明：`import`、`struct`、`def`、`extern def`。没有全局变量、没有顶层语句。

```aoxn
def main() -> int:      # 返回值是进程退出码
    return 0

def main2() -> void:    # 也可以声明 void：退出码 0
    return
```

- 程序必须有 `main`，否则报 `program has no 'main' function`。
- 每个文件里的名字（函数、结构体）共享一个命名空间，函数与结构体不能同名。

### 4. 绑定

```aoxn
x = 5                 # 推断为 int
y: float = 2.5        # 显式标注，必须与初值类型一致
x = x + 1             # 重赋值：类型必须保持 int
```

- 首次赋值即声明；类型由初值推断或由标注固定。
- 重赋值不能改变类型：`cannot assign a value of type string to 'x: int'`。
- **不允许遮蔽**（同一函数内重复声明同名变量、参数重名都是错误）。
- 赋值目标是 lvalue：变量、数组元素、结构体字段（可任意嵌套）。给非 lvalue 赋值报 `invalid assignment target`。
- 复合赋值（`+=` / `-=` 等）**不存在**。

### 5. 表达式与运算符

优先级从低到高（与实现一致）：

```text
or   := and ( ("or"  | "||") and )*
and  := eq  ( ("and" | "&&") eq )*
eq   := rel ( ("==" | "!=") rel )*
rel  := add ( ("<" | "<=" | ">" | ">=") add )*
add  := mul ( ("+" | "-") mul )*
mul  := unary ( ("*" | "/" | "%") unary )*
unary:= ("not" | "!" | "-") unary | postfix
postfix := primary ( "(" args ")" | "[" expr "]" | "." field )*
primary := INT | FLOAT | STRING | True | False | f"..." | IDENT | "(" expr ")" | "[" elems "]"
```

- `and` / `or` / `not` 与 `&&` / `||` / `!` 是**同义词**（词法阶段就归一到同一 token）；`True` / `False` 与 `true` / `false` 同理。
- `and` / `or` 短路求值（发射成 phi 节点）。
- 只有一元 `-`、`!`/`not`；**没有**一元 `+`，没有位运算，没有三元表达式。
- 只有**裸名字**可以调用：`f(x)` 合法，`obj.m(x)` 会报 `only named functions can be called`。

各运算符支持的运算对象（实测得出的边界）：

| 运算符 | 允许的类型 | 违反时的诊断 |
|---|---|---|
| `+` | (int,int)、(float,float)、(string,string) | `'+' requires two int or two float operands, found (int, float)` / `cannot concatenate string with int` |
| `-` `*` `/` | (int,int)、(float,float) | 同上（`*` 对字符串无效：`'*' requires two int or two float operands, found (string, int)`） |
| `%` | (int,int) 仅此 | `'%' requires two int operands, found (float, float)` |
| `<` `<=` `>` `>=` | (int,int)、(float,float)、(string,string) | 聚合类型不可比较 |
| `==` `!=` | 标量（含 bool）与 string | `cannot compare compound type [int; 2] with [int; 2]` |
| `not` / `!` | bool | `'!' requires bool, found int` |
| 一元 `-` | int、float | `unary '-' requires int or float, found bool` |

除法语义：`/` 对两个 `int` 是**截断整除**（`7 / 2` → `3`），对两个 `float` 是普通浮点除法（`7.0 / 2.0` → `3.500000`）。取模 `%` 仅限 `int`。

数组复制：`[e] * N`（或 `N * [e]`）要求字面量只有一个元素、`N` 是正的整数字面量，结果填进一个入口块分配的临时数组（运行时循环填充）。

### 6. 语句

| 语句 | 形式 |
|---|---|
| `if` / `elif` / `else` | 条件必须是 `bool`：`'if' condition must be bool, found int` |
| `while` | 唯一的通用循环；条件必须是 `bool` |
| `for x in range(...)` | `range(n)` / `range(a, b)` / `range(a, b, step)`，参数必须是 `int`；起止与步长在进入循环时求值一次 |
| `for x in arr` | 遍历数组元素，每个元素**拷贝**进 `x` |
| `break` / `continue` | 作用于最内层循环；在循环外报错 |
| `return` / `return expr` | 非 void 函数必须在**所有**路径返回值，否则 `function 'g' returns int but does not return a value on all paths` |
| `pass` | 显式空语句 |
| 赋值 / 表达式语句 | 见第 4 节 |

- `range` 是 `for` 上下文里的关键字，**不是函数**：在别处写 `range(3)` 会被当成未定义的名字。
- 步长为 0 会无限循环（不检查）。`for` 不能遍历字符串。
- `return` 之后还有语句报 `unreachable statement after 'return'`。

### 7. 数组

```aoxn
a = [10, 20, 30]                    # [int; 3]
xs: [int; 4] = [1, 2, 3, 4]         # 显式标注
m: [[int; 2]; 2] = [[1, 2], [3, 4]] # 嵌套
grid: [int; 6] = [0] * 6            # 复制填充
a[1] = 99                           # 元素赋值
print(len(a))                       # 3（编译期常量）
```

- 字面量不能为空（`empty array literals are not allowed (element type could not be inferred)`），所有元素必须同型。
- 下标是 `int`，不做边界检查；`len` 是编译期常量。
- 数组是**值类型**：赋值、传参、返回都整份拷贝（编译为 `memcpy`）。没有引用、指针或切片。
- 数组可以嵌套、可以放进结构体字段、可以作为参数与返回值（跨函数按指针 + 被调方拷贝的 ABI 传递，见 [代码生成与 LLVM](Codegen-and-LLVM-FFI.md)）。

### 8. 结构体

```aoxn
struct Point:
    x: int
    y: int

p = Point(x=1, y=2)     # 构造：全部字段、按名、顺序任意
p.x = 10                # 字段赋值
```

- 字段必须带类型标注；结构体可以**前向引用**（先用后定义也可）、可以含数组/其它结构体。
- 结构体**不能包含自己**（直接或间接），否则报错。
- 构造要求按名给出全部字段且不重复；字段名写错 → `struct 'P' has no field 'y'`，缺字段 → 编译错误。
- 结构体是值类型：`q = p` 之后改 `q.x` 不影响 `p`。
- 结构体与函数共用一个命名空间。
- 给普通函数传关键字实参会报错（关键字实参只用于结构体构造）。

### 9. 字符串

- 字符串是**不可变**字节序列，`len(s)` 是字节数（不是字符数）。
- `+` 拼接：运行时 `malloc` + 两次 `memcpy` + NUL。
- 六个比较运算符全部可用，逐字节字典序（`strcmp` 语义）。
- 可以放进结构体字段与数组元素；拷贝时共享底层字节（因为不可变，所以安全）。
- 拼接结果**永不释放**（目前没有 GC / arena 回收）：短命脚本无所谓，长跑服务请改用字节缓冲或 `Vec`（见 [标准库](Standard-Library.md)）。
- **没有**字符串索引/迭代（还没有 `char` 类型）；要按字节处理用 `str_get(s, i)` 这类 stdlib 辅助函数。

### 10. f-string

```aoxn
name = "Aoxn"
n = 3
print(f"hello {name}, {n * n}, {True}")   # hello Aoxn, 9, true
print(f"braces: {{literal}}")             # braces: {literal}
```

- `{expr}` 里的表达式类型必须是 `int` / `float` / `bool` / `string` 之一。
- `{{` 与 `}}` 输出字面花括号；单个 `}` 报错。
- 脱糖规则：`f"pre{expr}post"` → `"pre" + str(expr) + "post"`，所以 `str()` 的格式化行为就是 f-string 的行为（`float` 用 `%f`，6 位小数；`bool` 输出 `true` / `false`）。
- 插值里可以嵌套函数调用；如果要在插值里写字符串字面量，**必须用双引号**（词法扫描器只认双引号），例如
  `print(f"read={read_file("in.txt")}")`。
- **没有格式说明符，而且不会报错**：`f"{x:.2f}"`、`f"{x!r}"` 里的格式部分会被**静默忽略**，
  只按 `str(x)` 的默认格式输出（`1.500000`）。脱糖时每个插值只取第一个完整表达式、不检查剩余 token。
  只有像 `f"{x:@@@}"` 这种含非法字符的才会在词法阶段报 `unexpected character '@'`。
  格式说明符与多行 f-string 都还在路线图上（见 [路线图](Roadmap.md)）。

### 11. 函数

- 参数必须带类型标注；返回标注可省略（等价于 `void`）。
- 定义顺序无关；递归与相互递归都支持。
- 参数按值传递（标量是拷贝，聚合是"指针 + 被调方拷贝"的 ABI，语义上仍是拷贝）。
- 没有默认参数、可变参数、重载、闭包、匿名函数、函数指针。
- `print` / `len` / `str` 等内建名字不能被用户函数覆盖。

### 12. 泛型

```aoxn
def sort[T, N](arr: [T; N]) -> [T; N]:
    result = arr
    for i in range(N):
        for j in range(N - 1 - i):
            if result[j] > result[j + 1]:
                t = result[j]
                result[j] = result[j + 1]
                result[j + 1] = t
    return result
```

- 头部 `[T, N]` 声明类型参数与长度参数；调用 `sort([5, 3, 8, 1])` 时编译器**推断**出 `T = int, N = 4`，不需要写类型实参。
- 长度参数最多**一个**：`def f[T, N, M](a: [T; N], b: [T; M])` 报
  `only one length parameter is supported (found 'M' as well)`。
- 长度参数只能用在数组长度位置；没在头部声明的名字报
  `unknown array length 'N' (length parameters must be declared in the fn header, e.g. def f[T, N](arr: [T; N]))`。
- 单态化：每个不同的 `(T, N)` 组合生成一份实例，各自独立做类型检查；因此"`sort` 需要 `T` 上有 `>`"这件事是按实例判定的（`int` / `float` / `string` 可以，结构体不行）。
- 泛型函数不能是 `main`，也不能是 `extern`；声明本身不进入代码生成。
- 细节（实例命名、`call_map`、`GENERIC_LEN` 哨兵）见 [类型检查](Type-Checker.md)。

### 13. 内建函数

不需要声明、直接可用（按实现核对的签名）：

| 内建 | 签名 | 说明 |
|---|---|---|
| `print` | `print(x)` | `x` 是 `int` / `float` / `bool` / `string`；输出后换行 |
| `len` | `len(x) -> int` | `x` 是数组（编译期常量）或字符串（字节数） |
| `str` | `str(x) -> string` | `x` 是 `int` / `float` / `bool` / `string` |
| `load_i64` | `load_i64(addr: int) -> int` | 原始内存读（**一个**参数：偏移直接加在地址上） |
| `store_i64` | `store_i64(addr: int, v: int)` | 原始内存写 |
| `load_f64` | `load_f64(addr: int) -> float` | 原始内存读 |
| `store_f64` | `store_f64(addr: int, v: float)` | 原始内存写 |
| `load_u8` | `load_u8(base: int\|string, off: int) -> int` | 按字节读；`base` 可以是字符串 |
| `store_u8` | `store_u8(base: int\|string, off: int, v: int)` | 按字节写 |
| `as_string` | `as_string(p: int) -> string` | 把地址重新解释成字符串 |
| `as_ptr` | `as_ptr(s: string) -> int` | 把字符串取成地址 |
| `target_os` | `target_os() -> string` | **编译期**折叠成 `"windows"` / `"linux"` / `"macos"` / `"other"` |

原始内存一族是标准库与自举编译器的逃生舱：**完全不检查**（越界、悬垂、类型混淆都是未定义行为）。

### 14. 导入与多文件

```aoxn
import "../stdlib/stdlib.ax"
```

- 路径是字符串字面量，**相对于导入者所在文件**解析。
- 每个文件只被包含一次（按 canonical path 去重）；循环导入是编译错误。
- 所有文件合并进**同一个命名空间**：没有模块限定名（`lib.sort(...)`）、没有选择性导入、没有别名。
- `import` 只能出现在顶层。
- `aoxn run main.ax` 会自动解析全部传递导入；命令行给多个文件时也一样合并（但以字符串形式编译源码的 API 不支持 `import`）。

### 15. 外部函数与链接

```aoxn
extern def sqrt(x: float) -> float
extern def malloc(size: int) -> int
extern def fopen(path: string, mode: string) -> int
```

- `extern def` 没有函数体，链接时从默认库（C 运行时）解析。
- 参数与返回值走普通 C ABI：`int` ↔ `i64`、`float` ↔ `double`、`string` ↔ `char*`、指针用 `int` 承载。
- 限制：不能命名为 `main`（`'main' cannot be declared extern`）；不能是泛型（`extern functions cannot be generic`）。
- 需要额外库用 `-l NAME`（可重复）与 `-L DIR`（可重复），原样转发给 clang：

```powershell
cargo run -- run web\server_win.ax -l ws2_32
cargo run -- run selfhost\driver_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
```

- 平台上用 `target_os()` 分支（编译期常量），例如只在 Windows 调用 `_setmode`。

### 16. 严格性规则汇总

1. 无隐式 `int`/`float` 转换。
2. 条件必须是 `bool`。
3. 非 void 函数所有路径必须返回值。
4. 没有不可达语句。
5. 名字只能声明一次，类型一旦确定不能改（参数与局部变量都不允许遮蔽）。
6. 数组字面量非空且元素同型；结构体构造必须给全字段且按名。
7. 聚合不能比较、不能 `print`。
8. `break` / `continue` 只能在循环内；`return` 的类型必须匹配。
9. 顶层只能是声明。

这些限制是有意的：规则越少越确定，AI 生成的代码就越容易被机械验证。想放宽某条需要走语言提案（见 [开发指南](Development-Guide.md)）。

### 17. 明确不存在的特性

以下都**没有**实现（部分在路线图上，见 [路线图](Roadmap.md)）：

| 类别 | 缺失的东西 |
|---|---|
| 类型 | `enum` / `match`、`char`、无符号整数、元组、联合类型、类型别名、可选类型 / `None` |
| 内存 | 引用 / 指针类型、手动 `free` 的字符串、GC 或 arena 回收、所有权 / 借用 |
| 字符串 | 索引、迭代、切片、格式说明符、原始字符串、单引号 |
| 表达式 | 位运算、复合赋值、三元表达式、一元 `+`、lambda / 闭包、`assert` |
| 语句 | 顶层语句（隐式 `main`）、`try` / 异常、`with`、多返回值、装饰器 |
| 模块 | 模块限定名、选择性导入、别名导入、包管理 |
| 函数 | 默认参数、可变参数、重载、函数指针 / 一等函数、运算符重载、泛型约束 |
| 工具链 | `argv`（所以自举编译器还不能移植 CLI 语义）、调试信息 / 断点、增量编译 |
| 平台 | 交叉编译、MinGW、32 位目标（见 [平台支持](Platform-Support.md)） |

### 18. 相关页面

- 语法教学：[语言速览](Language-Tour.md)
- 检查器内部与单态化细节：[类型检查](Type-Checker.md)
- 标准库清单：[标准库](Standard-Library.md)
- 类型/内存布局怎么落到 LLVM：[代码生成与 LLVM](Codegen-and-LLVM-FFI.md)

## English

This page follows the implementation in `src/parser.rs` + `src/typecheck.rs` at
v0.26.3 and quotes diagnostics that were actually observed. The normative
specification text is [`docs/spec.md`](../docs/spec.md) (labelled v0.9, a few
sections outdated — the implementation wins).

### 1. Lexical structure and layout

| Rule | Detail |
|---|---|
| Blocks | Defined by **indentation**, not braces. Any consistent width; a tab counts as 4 columns |
| Statement end | A line break ends the statement; there are **no semicolons** (`;` appears only inside `[T; N]` types) |
| Comments | `#` to end of line. Blank and comment-only lines produce no tokens and do not affect indentation |
| Continuation | Line breaks inside `(...)` are ignored (implicit joining); anywhere else a break ends the statement |
| One-line blocks | `if n < 2: return n` is legal (`:` may be followed by **one simple statement**) |
| Compound statements | Cannot follow `:` on the same line (`if` / `while` / `for` / `def` / `struct` report `compound statements cannot appear on the same line after ':'`) |
| f-strings | `f"..."`, `{expr}` interpolation, `{{` / `}}` for literal braces |
| String literals | **Double quotes only** — no single quotes, no raw strings |
| Escapes | `\n` `\t` `\\` `\"` (no `\r`, no hex/Unicode escapes) |
| Numeric literals | Decimal integers and floats (floats need the dot); **no hex/binary literals** |
| Comment trap | `//` is neither a comment nor an operator: `5 // 2` gives `expected an expression, found Slash` |

An indentation mismatch reports `unindent does not match any outer indentation level`.

### 2. Types

| Type | Meaning | Backend |
|---|---|---|
| `int` | signed 64-bit integer | i64 |
| `float` | 64-bit IEEE float | double |
| `bool` | `True` / `False` (and `true` / `false`) | i1 |
| `string` | immutable NUL-terminated byte sequence | pointer to bytes |
| `void` | no value (return types only) | none |
| `[T; N]` | fixed-size array, `N > 0` integer literal | `[N x T]` |
| `Name` | struct | named struct |

Deliberately unsafe-but-fast semantics (identical to C, **by design**):

- `int` and `float` **never** convert implicitly (not even literals).
- Signed integer overflow is undefined behaviour (emitted with `nsw`).
- Division by zero is undefined behaviour (no runtime check).
- Array indexing is **unchecked** (C-style).
- The raw-memory builtins (section 13) check nothing at all.

### 3. Program structure

Only four declarations are allowed at the top level: `import`, `struct`,
`def`, `extern def`. There are no globals and no top-level statements.

```aoxn
def main() -> int:      # the return value is the process exit code
    return 0

def main2() -> void:    # a void main exits with 0
    return
```

- A program must have `main`, otherwise `program has no 'main' function`.
- Functions and structs of a file share one namespace; a function and a struct
  cannot share a name.

### 4. Bindings

```aoxn
x = 5                 # inferred int
y: float = 2.5        # annotated; must match the initializer
x = x + 1             # re-assignment keeps the type
```

- The first assignment declares the name; the type comes from the initializer or
  the annotation.
- Re-assignment cannot change the type:
  `cannot assign a value of type string to 'x: int'`.
- **Shadowing is not allowed** (redeclaring a name, or a parameter, is an error).
- Assignment targets are lvalues: variables, array elements, struct fields
  (arbitrarily nested). Assigning to anything else reports
  `invalid assignment target`.
- Compound assignment (`+=`, `-=`, …) **does not exist**.

### 5. Expressions and operators

Precedence, lowest to highest (as implemented):

```text
or   := and ( ("or"  | "||") and )*
and  := eq  ( ("and" | "&&") eq )*
eq   := rel ( ("==" | "!=") rel )*
rel  := add ( ("<" | "<=" | ">" | ">=") add )*
add  := mul ( ("+" | "-") mul )*
mul  := unary ( ("*" | "/" | "%") unary )*
unary:= ("not" | "!" | "-") unary | postfix
postfix := primary ( "(" args ")" | "[" expr "]" | "." field )*
primary := INT | FLOAT | STRING | True | False | f"..." | IDENT | "(" expr ")" | "[" elems "]"
```

- `and` / `or` / `not` and `&&` / `||` / `!` are **synonyms** (normalized to the
  same token during lexing); likewise `True` / `False` and `true` / `false`.
- `and` / `or` short-circuit (they emit phi nodes).
- Unary minus and `!`/`not` only: **no** unary plus, no bitwise operators, no
  ternary expression.
- Only **bare names** are callable: `f(x)` works, `obj.m(x)` reports
  `only named functions can be called`.

Operand types per operator (observed boundaries):

| Operator | Allowed operands | Diagnostic when violated |
|---|---|---|
| `+` | (int,int), (float,float), (string,string) | `'+' requires two int or two float operands, found (int, float)` / `cannot concatenate string with int` |
| `-` `*` `/` | (int,int), (float,float) | same as above (`*` on a string: `'*' requires two int or two float operands, found (string, int)`) |
| `%` | (int,int) only | `'%' requires two int operands, found (float, float)` |
| `<` `<=` `>` `>=` | (int,int), (float,float), (string,string) | aggregates cannot be compared |
| `==` `!=` | scalars (including bool) and string | `cannot compare compound type [int; 2] with [int; 2]` |
| `not` / `!` | bool | `'!' requires bool, found int` |
| unary `-` | int, float | `unary '-' requires int or float, found bool` |

Division semantics: `/` on two `int`s is **truncating integer division**
(`7 / 2` → `3`), on two `float`s it is ordinary floating-point division
(`7.0 / 2.0` → `3.500000`). `%` is int-only.

Array replication: `[e] * N` (or `N * [e]`) requires a single-element literal and
a positive integer literal `N`; the result is an entry-hoisted temporary filled
by a runtime loop.

### 6. Statements

| Statement | Form |
|---|---|
| `if` / `elif` / `else` | the condition must be `bool`: `'if' condition must be bool, found int` |
| `while` | the general loop; the condition must be `bool` |
| `for x in range(...)` | `range(n)` / `range(a, b)` / `range(a, b, step)` with `int` arguments; bounds and step are evaluated once at loop entry |
| `for x in arr` | iterates array elements, each **copied** into `x` |
| `break` / `continue` | affect the innermost loop; outside a loop they are errors |
| `return` / `return expr` | non-void functions must return on **every** path, else `function 'g' returns int but does not return a value on all paths` |
| `pass` | explicit empty statement |
| assignment / expression statement | see section 4 |

- `range` is a `for`-context keyword, **not a function**: writing `range(3)`
  elsewhere resolves as an undefined name.
- A zero step loops forever (unchecked). `for` cannot iterate a string.
- A statement after `return` reports `unreachable statement after 'return'`.

### 7. Arrays

```aoxn
a = [10, 20, 30]                    # [int; 3]
xs: [int; 4] = [1, 2, 3, 4]         # annotated
m: [[int; 2]; 2] = [[1, 2], [3, 4]] # nested
grid: [int; 6] = [0] * 6            # replication
a[1] = 99                           # element assignment
print(len(a))                       # 3 (compile-time constant)
```

- Literals cannot be empty
  (`empty array literals are not allowed (element type could not be inferred)`)
  and all elements must share one type.
- The index is an `int`, unchecked; `len` is a compile-time constant.
- Arrays are **value types**: assignment, parameter passing and returns copy the
  whole array (lowered to `memcpy`). There are no references, pointers or slices.
- Arrays nest, live in struct fields, and can be parameters and return values
  (across functions they travel as a pointer the callee copies — see
  [Codegen and LLVM](Codegen-and-LLVM-FFI.md)).

### 8. Structs

```aoxn
struct Point:
    x: int
    y: int

p = Point(x=1, y=2)     # every field, by name, any order
p.x = 10                # field assignment
```

- Fields need type annotations; structs may **forward-reference** each other,
  nest, and contain arrays.
- A struct **cannot contain itself**, directly or indirectly.
- Construction requires every field exactly once, by name; an unknown field
  gives `struct 'P' has no field 'y'`, a missing one is an error too.
- Structs are value types: after `q = p`, changing `q.x` does not touch `p`.
- Structs and functions share one namespace.
- Keyword arguments on ordinary functions are rejected (they are for struct
  construction only).

### 9. Strings

- Strings are **immutable** byte sequences; `len(s)` is the byte count, not the
  character count.
- `+` concatenates: runtime `malloc` + two `memcpy`s + NUL.
- All six comparisons work, byte-wise lexicographic (`strcmp` semantics).
- They can live in struct fields and array elements; copies share the underlying
  bytes (sound because strings never change).
- Concatenation results are **never freed** (no GC / arena reclamation yet):
  fine for short scripts, wrong for a long-running service — render into byte
  buffers or `Vec` instead (see [Standard Library](Standard-Library.md)).
- There is **no** string indexing or iteration (no `char` type yet); use stdlib
  helpers such as `str_get(s, i)` for bytes.

### 10. f-strings

```aoxn
name = "Aoxn"
n = 3
print(f"hello {name}, {n * n}, {True}")   # hello Aoxn, 9, true
print(f"braces: {{literal}}")             # braces: {literal}
```

- The expression in `{...}` must be `int` / `float` / `bool` / `string`.
- `{{` and `}}` produce literal braces; a single `}` is an error.
- Desugaring: `f"pre{expr}post"` → `"pre" + str(expr) + "post"`, so `str()`
  defines f-string formatting (`float` via `%f` with 6 decimals; `bool` as
  `true` / `false`).
- Interpolations may contain nested calls; a string literal inside an
  interpolation **must use double quotes** (the scanner only understands those),
  e.g. `print(f"read={read_file("in.txt")}")`.
- **There are no format specifiers, and no diagnostic either**: a format part such
  as `f"{x:.2f}"` or `f"{x!r}"` is **silently ignored** and the value prints with
  `str(x)`'s default formatting (`1.500000`) — desugaring takes only the first
  complete expression per interpolation and never inspects the leftover tokens.
  Only illegal characters such as `f"{x:@@@}"` fail, at the lexer, with
  `unexpected character '@'`. Format specifiers and multi-line f-strings are both
  still on the roadmap (see [Roadmap](Roadmap.md)).

### 11. Functions

- Parameters require type annotations; the return annotation is optional and
  defaults to `void`.
- Definition order does not matter; recursion and mutual recursion work.
- Parameters are passed by value (scalars copied; aggregates use the "pointer
  plus callee copy" ABI, semantically still a copy).
- No default arguments, varargs, overloading, closures, lambdas or function
  pointers.
- Builtins such as `print` / `len` / `str` cannot be shadowed by user functions.

### 12. Generics

```aoxn
def sort[T, N](arr: [T; N]) -> [T; N]:
    result = arr
    for i in range(N):
        for j in range(N - 1 - i):
            if result[j] > result[j + 1]:
                t = result[j]
                result[j] = result[j + 1]
                result[j + 1] = t
    return result
```

- The header `[T, N]` declares type parameters and one length parameter; at a
  call site such as `sort([5, 3, 8, 1])` the compiler **infers** `T = int,
  N = 4` — no explicit type arguments.
- At most **one** length parameter:
  `def f[T, N, M](a: [T; N], b: [T; M])` reports
  `only one length parameter is supported (found 'M' as well)`.
- A length parameter is only valid in an array-length position; an undeclared
  name there reports
  `unknown array length 'N' (length parameters must be declared in the fn header, e.g. def f[T, N](arr: [T; N]))`.
- Monomorphization: every distinct `(T, N)` combination gets its own instance,
  each checked independently — which is why "`sort` needs `>` on `T`" is decided
  per instance (`int` / `float` / `string` work, structs do not).
- A generic function cannot be `main` and cannot be `extern`; generic
  declarations never reach code generation.
- Internals (instance naming, `call_map`, the `GENERIC_LEN` sentinel) are in
  [Type Checker](Type-Checker.md).

### 13. Builtins

Available without declaration (signatures verified against the implementation):

| Builtin | Signature | Notes |
|---|---|---|
| `print` | `print(x)` | `x` is `int` / `float` / `bool` / `string`; appends a newline |
| `len` | `len(x) -> int` | array (compile-time constant) or string (bytes) |
| `str` | `str(x) -> string` | `x` is `int` / `float` / `bool` / `string` |
| `load_i64` | `load_i64(addr: int) -> int` | raw memory read (**one** argument: add offsets to the address) |
| `store_i64` | `store_i64(addr: int, v: int)` | raw memory write |
| `load_f64` | `load_f64(addr: int) -> float` | raw memory read |
| `store_f64` | `store_f64(addr: int, v: float)` | raw memory write |
| `load_u8` | `load_u8(base: int\|string, off: int) -> int` | byte read; `base` may be a string |
| `store_u8` | `store_u8(base: int\|string, off: int, v: int)` | byte write |
| `as_string` | `as_string(p: int) -> string` | reinterpret an address as a string |
| `as_ptr` | `as_ptr(s: string) -> int` | take a string's address |
| `target_os` | `target_os() -> string` | **compile-time** constant: `"windows"` / `"linux"` / `"macos"` / `"other"` |

The raw-memory family is the escape hatch for the standard library and the
self-hosted compiler: it checks **nothing** (out of bounds, dangling pointers and
type confusion are all undefined behaviour).

### 14. Imports and multiple files

```aoxn
import "../stdlib/stdlib.ax"
```

- The path is a string literal resolved **relative to the importing file**.
- Each file is included exactly once (deduplicated by canonical path); import
  cycles are compile errors.
- All files merge into **one namespace**: no module qualification
  (`lib.sort(...)`), no selective imports, no aliases.
- `import` is top-level only.
- `aoxn run main.ax` resolves all transitive imports; passing several files on
  the command line merges them the same way (string-based source APIs do not
  support `import`).

### 15. Extern functions and linking

```aoxn
extern def sqrt(x: float) -> float
extern def malloc(size: int) -> int
extern def fopen(path: string, mode: string) -> int
```

- `extern def` has no body and is resolved at link time from the default
  libraries (the C runtime).
- Parameters and returns use the plain C ABI: `int` ↔ `i64`, `float` ↔ `double`,
  `string` ↔ `char*`, and pointers travel as `int`.
- Restrictions: cannot be named `main` (`'main' cannot be declared extern`) and
  cannot be generic (`extern functions cannot be generic`).
- Extra libraries go through `-l NAME` and `-L DIR` (both repeatable), forwarded
  verbatim to clang:

```powershell
cargo run -- run web\server_win.ax -l ws2_32
cargo run -- run selfhost\driver_demo.ax -l LLVM-C -L "C:\Program Files\LLVM\lib"
```

- Branch on the platform with `target_os()` (a compile-time constant), for
  example to call `_setmode` on Windows only.

### 16. The strict rules, collected

1. No implicit `int`/`float` conversions.
2. Conditions must be `bool`.
3. Non-void functions must return on all paths.
4. No unreachable statements.
5. A name is declared once and keeps its type (no shadowing, parameters included).
6. Array literals are non-empty and same-typed; struct construction must name
   every field.
7. Aggregates cannot be compared or printed.
8. `break` / `continue` only inside a loop; `return` types must match.
9. Top level is declarations only.

The restrictions are the point: fewer rules make the language deterministic and
make AI-generated code mechanically verifiable. Relaxing one is a language
proposal, not a drive-by patch (see the [Development Guide](Development-Guide.md)).

### 17. Features that deliberately do not exist

None of the following is implemented (some are on the roadmap — see
[Roadmap](Roadmap.md)):

| Area | Missing |
|---|---|
| Types | `enum` / `match`, `char`, unsigned integers, tuples, unions, type aliases, optionals / `None` |
| Memory | reference/pointer types, freeing strings, GC or arena reclamation, ownership/borrowing |
| Strings | indexing, iteration, slicing, format specifiers, raw strings, single quotes |
| Expressions | bitwise operators, compound assignment, ternary, unary `+`, lambdas/closures, `assert` |
| Statements | top-level statements (implicit `main`), `try`/exceptions, `with`, multiple return values, decorators |
| Modules | module qualification, selective imports, aliased imports, a package manager |
| Functions | default arguments, varargs, overloading, function pointers / first-class functions, operator overloading, generic constraints |
| Tooling | `argv` (which is why the self-hosted compiler cannot port the CLI yet), debug info/breakpoints, incremental compilation |
| Platforms | cross-compilation, MinGW, 32-bit targets (see [Platform Support](Platform-Support.md)) |

### 18. Related pages

- Tutorial syntax tour: [Language Tour](Language-Tour.md)
- Checker internals and monomorphization: [Type Checker](Type-Checker.md)
- Standard library inventory: [Standard Library](Standard-Library.md)
- How types and layouts reach LLVM: [Codegen and LLVM](Codegen-and-LLVM-FFI.md)

---

## 源文件 / Source files

- [src/parser.rs](../src/parser.rs) — grammar, precedence, layout, generic headers (authoritative for this page)
- [src/typecheck.rs](../src/typecheck.rs) — strict rules, builtin signatures, operator type rules
- [src/lexer.rs](../src/lexer.rs) — tokens, f-string scanning, escapes, layout
- [src/ast.rs](../src/ast.rs) — AST shapes and the `GENERIC_LEN` sentinel
- [docs/spec.md](../docs/spec.md) — the normative specification text (v0.9; outdated in places)
- [stdlib/stdlib.ax](../stdlib/stdlib.ax) — real uses of externs, raw memory and generics
- [tests/pipeline.rs](../tests/pipeline.rs) — every rule above is pinned by a test
