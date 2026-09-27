# 类型检查器 · Type Checker

> **中文**：`src/typecheck.rs` 的严格检查器：五趟流程、`FnSig`/`StructInfo` 符号表、逐条语义规则与诊断原文，以及 `[T; N]` 泛型的单态化机制与它交给 codegen 的契约。
> **English**: The strict checker in `src/typecheck.rs`: its five-pass pipeline, the `FnSig`/`StructInfo` symbol tables, every semantic rule with its verbatim diagnostic, the monomorphizer behind `[T; N]` generics, and the contract it hands to codegen.

## 中文

### 0. 先自查：什么时候会被拒绝

`typecheck.rs` 的立场是"能静态确定的，绝不推到运行期"：没有隐式转换、没有重载解析、没有宽容的
数据流推断。下面是最常见的拒绝理由（每条都能在 §3 找到规则与诊断原文）：

| 你写的 | 结果 |
|---|---|
| `x: int = 1 + 1.5` | `'+' requires two int or two float operands, found (int, float)` |
| `if 1:` / `while 1:` | 条件必须是 `bool` |
| 非 `void` 函数有一条路径没 `return` | `... does not return a value on all paths` |
| `return` 之后还有语句 | `unreachable statement after 'return'` |
| 同一个名字第二次绑定成别的类型 | `cannot re-declare ...` / `cannot assign a value of type ...` |
| `[]` 或 `[1, 2.5]` | 空数组字面量 / 元素类型不一致 |
| 结构体少字段、字段写两次、字段名拼错 | `missing field(s)` / `given more than once` / `has no field` |
| `print(数组或结构体)` | `print requires int, float, bool, or string` |
| `len(3)` | `len requires an array or string` |
| `def main[T]` 或 `extern def main` | 泛型 / extern 都不能是 `main` |
| 非泛型签名里写 `[int; N]` | parse 阶段就报 `unknown array length 'N'` |
| 泛型的长度参数在实参里推不出来 | `cannot infer array length N` |

两个容易误判的点：**未使用的名字（局部变量、参数、函数）不报错**；**从未被调用的泛型函数，其函数体
根本不会被检查**（见 §7）。

### 1. 五趟流程

`check(&Program)` 的顺序（`src/typecheck.rs:77`）：

| 趟 | 做什么 | 关键点 |
|---|---|---|
| 0 | `collect_structs` 建 `StructTable` | 先占名（允许结构体前向引用）→ 解析字段类型 → DFS 查自包含 |
| 1 | 具体函数签名 | 先登记全部 `FnSig`，所以递归/互递归与声明顺序无关；顺带查参数、返回类型、`void` 参数、重名 |
| 2 | 泛型声明登记 | 只登记 `FnDecl`，**不检查函数体**（函数体等实例化） |
| 3 | 检查具体（非 extern、非泛型）函数体 | 顺序即声明顺序；每个函数设置 `cur_file`，诊断因此落在正确的文件 |
| 4 | 排空实例队列 | FIFO；实例体里还能再入队新实例（嵌套泛型）；每个实例**就是**将来 codegen 发射的那个对象 |

`main` 的存在性在趟 2 之后、趟 3 之前检查（`program has no 'main' function`），所以"没有 `main`"
会盖住函数体里的类型错误。

### 2. 类型环境与符号表

- **`Type`**（`src/ast.rs`）：`int | float | bool | string | void | [T; N] | Name`。相等就是结构相等，
  没有子类型、没有别名、没有隐式转换。
- **`FnSig { params: Vec<Type>, ret: Type }`**：只登记具体函数，按名字放进 `sigs`（`FastBuild` 哈希器）。
  泛型声明单独放 `generics`，两表互斥，所以同一个名字在两张表里不可能同时出现。
- **`StructTable = HashMap<String, StructInfo>`**；v0.26.3 起 `StructInfo { fields: Vec<(String, Type)>, index }`，
  `field_type()` 走 `index` 查表 → 字段查找 O(1)（40 字段的状态结构体上，原来的线性扫描是可测的开销）。
- **结构体与函数共用一个命名空间**：调用 `P(x=1)` 时按 `sigs` → `generics` → `structs` 的顺序解析；
  命中结构体就走"具名字段构造"，三者都没有才报 `call to undefined function or struct 'P'`。
- **作用域**：一个函数一个**扁平** `HashMap<String, Type>`，参数先放进去；`if`/`while`/`for` 的块
  **不压栈、不弹出**。所以绑定是函数级的（Python 风格）：块内 `let` 的新名字在块外依然可见（实测
  `if True: z = 1` 之后 `print(z)` 能编译），也**不存在遮蔽**——第二次绑定同名就是重赋值，类型必须一致。
- **诊断文件**：`Tc::cur_file` 跟着"正在检查的函数"走；`Diag::at("type", file, line, col, msg)` 里的
  `file` 是 `src/files.rs` 注册表下标，打印/JSON 时才解析成路径。跨文件编译时错误因此指向真正出错的
  那个文件（`import_error_reports_importing_file` 保护这一点）。

### 3. 严格语义规则清单

#### 3.1 运算符与表达式

| 规则 | 违反时的诊断（逐字） |
|---|---|
| `+ - * /` 只接受"两个 int"或"两个 float" | `'+' requires two int or two float operands, found (int, float)`（`-`/`*`/`/` 同格式） |
| `%` 只接受两个 int | `'%' requires two int operands, found (float, float)` |
| `+` 对 string 是拼接，只能 string + string | `cannot concatenate string with int` |
| `==`/`!=` 两侧类型必须相同且非 void | `cannot compare float with int` |
| 聚合（数组/结构体）不能比较 | `cannot compare compound type P with P` |
| `< <= > >=` 只接受 (int,int)/(float,float)/(string,string) | `'<' requires two int, two float, or two string operands, found (P, P)` |
| `and`/`or`（`&&`/`||`）两侧必须 bool | `'&&' requires bool operands, found (int, int)` |
| `not`（`!`）只接受 bool | `'!' requires bool, found int` |
| 一元 `-` 只接受 int/float | `unary '-' requires int or float, found string` |
| 下标必须是 int | `array index must be int, found float` |
| 只能索引数组（string/结构体/int 都不行） | 表达式位置：`cannot index a value of type string (only [T; N] arrays are indexable)`；赋值目标位置是同一句的前半段 |

注意 `op_str` 把 `and`/`or` 也印成 `&&`/`||`：源码写 `1 and 2`，诊断里是 `'&&' requires bool operands`。

#### 3.2 条件与循环

| 规则 | 违反时的诊断 |
|---|---|
| `if` 条件必须 bool | `'if' condition must be bool, found int` |
| `while` 条件必须 bool | `'while' condition must be bool, found int` |
| `range` 收 1..3 个 int 实参 | `range expects 1 to 3 arguments, found 0` / `range arguments must be int, found float` |
| `for x in ...` 只能遍历数组（string 也不行） | `'for' can only iterate over arrays, found string` |
| `break`/`continue` 必须在循环内 | `'break' outside of a loop` / `'continue' outside of a loop` |
| 循环变量沿用普通绑定规则 | `loop variable 'x' is already int, cannot reuse as float` |

#### 3.3 返回与可达性

| 规则 | 违反时的诊断 |
|---|---|
| 非 void 函数必须在所有路径返回值 | `function 'f' returns int but does not return a value on all paths` |
| 该返回值时不能裸 `return` | `'return' must return a value of type int` |
| void 函数不能返回值 | `void function cannot return a value` |
| 返回类型必须精确相等 | `'return' type mismatch: expected int, found float` |
| `return` 之后同块内不得再有语句 | `unreachable statement after 'return'` |

"保证返回"的判定很保守，只有两种语句算数（`stmt_guarantees_return`）：`return`，以及 `then`/`else`
**两个块都**保证返回的 `if`（`elif` 链在解析期折叠成嵌套 `if`，所以全覆盖的 `if/elif/else` 也算）。
**`while True:` 不算**——下面这段会被拒绝：

```aoxn
def f() -> int:
    while True:
        return 1        # 编译期报错：function 'f' returns int but does not return a value on all paths

def main() -> int:
    print(f())
    return 0
```

这是刻意的保守分析：检查器只看语句形状，不做循环是否终止的推理。

不可达检查在**块内向前**：一旦某语句保证返回，同块后续任何语句都报错；`if/elif/else` 全返回之后继续
写语句同样报错（实测 `unreachable statement after 'return'`）。

#### 3.4 绑定与赋值

| 规则 | 违反时的诊断 |
|---|---|
| 首次绑定：注解必须与初始化表达式类型一致 | `cannot initialize 'x: int' with an expression of type float` |
| 不能绑定 void 表达式 | `cannot bind a void expression to 'x'` |
| 重赋值/重声明：类型固定 | 带注解：`cannot re-declare 'x' as float: it is already int`；不带注解：`cannot assign a value of type string to 'x: int'` |
| 复合目标（下标/字段）赋值：目标类型固定 | `cannot assign a value of type float to a target of type int` |
| 赋值目标必须是变量/下标/字段 | 解析阶段：`invalid assignment target` |
| 不能给未声明的名字赋值 | 复合目标先报 `unknown variable 'y'`（裸名字 `y = ...` 是绑定，不是赋值） |

参数天然参与同一张表：在函数体里给参数换成别的类型，就是一次普通的重赋值错误。

#### 3.5 数组

| 规则 | 违反时的诊断 |
|---|---|
| 字面量必须非空 | parse 阶段：`empty array literals are not allowed (element type could not be inferred)` |
| 元素同型（第一个元素定类型） | `array literal elements must share one type: found int and float` |
| 长度必须是正整数字面量 | parse 阶段：`array length must be a positive integer` |
| 复制的计数必须为正 | 检查器里有 `array replication count must be positive`，但解析器只对"单元素字面量 × 正整数字面量"构造复制节点（见 §7） |
| 下标必须 int、只能索引数组 | 见 3.1 |

`len(x)` 返回**静态**长度，类型是 int。数组下标不做边界检查（C 风格，越界是未定义行为）。

#### 3.6 结构体

| 规则 | 违反时的诊断 |
|---|---|
| 字段名唯一 | `duplicate field 'x' in struct 'P'` |
| 字段类型必须存在 | `unknown type 'Foo'` |
| 字段不能是 void | `field 'x' cannot have type void` |
| 不能自包含 | `recursive struct 'Node' (a struct cannot contain itself, directly or indirectly)` |
| 构造必须全部具名 | `struct 'P' must be constructed with named fields: P(field=value, ...)` |
| 每个字段恰好一次、名字必须存在、类型精确 | 缺失：`struct literal 'P' is missing field(s): y`；重复：`field 'x' given more than once in 'P'`；未知：`struct 'P' has no field 'z'`；类型不符：`field 'x' of 'P' must be int, found float` |
| 字段访问必须存在 | `type P has no field 'z'` |
| 结构体不能重名 | `struct 'P' is defined more than once`；与函数同名时看谁后定义：`'P' is already defined as a struct` |

字段顺序无关（任意顺序、构造时按名匹配）；结构体可前向引用（先占名、再解析字段类型）。

#### 3.7 内建函数

| 规则 | 违反时的诊断 |
|---|---|
| `print` 一个位置参数，只接受 int/float/bool/string | `print expects exactly 1 positional argument` / `print requires int, float, bool, or string, found [int; 2]` |
| `len` 一个位置参数，只接受数组/string | `len expects exactly 1 positional argument` / `len requires an array or string, found int` |
| `str` 一个位置参数，可打印类型 | `str expects exactly 1 positional argument` / `cannot convert P to string`（f-string 走的就是 `str()`） |
| 具名实参只属于结构体构造 | `function 'id' takes positional arguments only` |

`load_i64`/`store_u8`/`as_ptr`/`as_string`/`target_os` 等内建也在这一层检查：签名与错误信息在
`check_expr` 的 `Expr::Call` 分支里逐条写死，不由语言层面的函数表提供。

#### 3.8 命名与定义

| 规则 | 违反时的诊断 |
|---|---|
| 函数不能重名（泛型与具体函数共享名字空间） | `function 'f' is defined more than once` |
| 参数名不能重复 | `duplicate parameter 'x' in function 'f'` |
| 参数必须有类型注解、不能是 void | `parameter 'x' cannot have type void` |
| 类型名必须存在（被显式解析的位置） | `unknown type 'Foo'` |
| 必须有 `main` | `program has no 'main' function` |
| `main` 不能是 extern | `'main' cannot be declared extern` |
| 用到的名字必须已绑定 | `unknown variable 'x'` |
| 调用必须可解析 | `call to undefined function or struct 'g'` |
| 实参个数与类型 | `function 'f' expects 1 argument(s), found 2` / `argument 1 of 'f' must be int, found float` |

### 4. 泛型单态化

Aoxn 的泛型没有运行期表示：每个不同的 `(T, N)` 组合在编译期生成一份具体实例，codegen 永远看不到
泛型声明本身。

#### 4.1 语法与哨兵

```aoxn
def sort[T, N](arr: [T; N]) -> [T; N]:
    result = arr
    return result
```

（上面是片段，只展示泛型头部与函数体的形状；可运行的完整程序见 §4.6。）

- 头部 `[T, N]` 里的名字全部进 `type_params`；出现在数组长度位置的类型参数进 `len_param`（只取
  **第一个**，多于一个由解析器报 `only one length parameter is supported (found 'M' as well)`）。
- `[T; N]` 里的 `N` 被解析成 **`GENERIC_LEN = usize::MAX`** 哨兵（`src/ast.rs:63`）；`Type` 的 `Display`
  会把它印回成 `[T; N]`，所以诊断里看到的仍是源码形态。
- 在**非泛型**函数里写 `[int; N]`，parse 阶段就报
  `unknown array length 'N' (length parameters must be declared in the fn header, e.g. def f[T, N](arr: [T; N]))`；
  检查器里还有一道针对 `GENERIC_LEN` 的兜底：`array length parameters are only valid inside generic functions`。
- 泛型函数不能是 `main`（`'main' cannot be generic`），也不能是 `extern`
  （`extern functions cannot be generic`）。

#### 4.2 一个调用点做五件事（`instantiate_call`）

1. **检查实参**，拿到每个实参的类型；出现具名实参直接报 `takes positional arguments only`，个数不符报
   `function '{name}' expects {n} argument(s), found {m}`。
2. **统一（unify）**：把声明参数类型（可能含 `T`、`[T; N]`）与实参类型逐一对上。
   - `T` 绑定进 `subst_t`；同一趟里第二次出现必须与第一次**相同**（`pair(1, True)` 就死在这里）。
   - `[.. ; N]` 绑定长度 `n`；同一趟里第二次出现长度必须一致。
   - 固定长度数组要求长度相等，然后递归比较元素类型；其它类型是**精确相等**。
   - 任一条不满足即报 `argument {i} of '{name}' must be {声明的参数类型原文}, found {实参类型}`。
3. **替换返回类型**：`subst_type(ret)`；若 `ret` 还需要 `N` 而这趟没推出来 →
   `cannot infer array length N (use it in a parameter)`。
4. **命名并登记路由**：`mangle(...)` 算出实例名，然后 `call_map.insert(调用表达式节点地址, 实例名)`。
5. **首次见到的实例入队**：克隆泛型声明 → 改名 → 清空 `type_params` → 替换参数类型、返回类型，以及
   **整个函数体**（`subst_block_types`：`let` 注解与表达式里的 `T`/`N` 全部落地）。函数体里的 `N` 会被
   换成 `Expr::Int(长度)`，位置是合成的（`file = u32::MAX`），所以实例体内的 `N` 就是一个编译期 int 常量，
   可以直接写 `range(N)`、`N - 1`、`arr[N - 1]`。

趟 4 再逐个检查排队的实例体——用**完全相同的规则**，因此实例体里的嵌套泛型调用会产生新实例，队列继续
增长，直到排空。

#### 4.3 实例命名

`mangle`（`src/typecheck.rs:1003`）的形状是 `名字` + 每个声明的类型参数一个 `.slug` +（若绑定了长度）`.长度`：

| 类型 | slug |
|---|---|
| int / float / bool / string / void | `i` / `f` / `b` / `s` / `v` |
| `[E; L]` | `a<元素 slug>x<L>` |
| 结构体 `P` | `s_P` |
| 该类型参数没有被约束（用不上） | `?` |

实测（`aoxn ir` 打印的函数名）：

```text
id(1), id(2)              -> id.i              两个同型调用共享同一个实例
id(3.5), id(4.5)          -> id.f
first([10, 20, 30])       -> first.i.?.3       T=int；长度参数在类型位上是 ".?"，绑定的长度追加为 3
first([1.5, 2.5])         -> first.f.?.2
first(["a", "b", "c"])    -> first.s.?.3
last([4, 5, 6, 7])        -> last.i.?.4
sum_int([1, 2, 3])        -> sum_int.?.3       只声明了长度参数 N，没有 T
unused_param[T](x: int)   -> unused_param.?
```

`first.i.?.3` 里的 `.?` 值得解释：长度参数 `N` 在 `type_params` 里同样占一个槽位，但它不参与类型替换，
所以槽位是 `?`，真正的长度追加在最后。这套命名在 Aoxn 自举的类型检查器里同样成立：
`selfhost/tycheck_demo.ax` 的输出由 `selfhost_typechecker_accepts_and_rejects` 逐字断言，里面就有
`wrap.i`、`wrap.b`、`id.i`、`id.b`、`first.i.?.3`。

#### 4.4 去重与顺序

- `done: HashSet<String>` 按**修饰名**去重：同一个 `(T, N)` 只克隆、检查、发射一次，多个调用点共享它。
- `queue: VecDeque<FnDecl>` 是 FIFO（v0.26.3 从 `Vec::remove(0)` 改过来——旧写法在实例数量上是 O(m²)）。
- 队列顺序 = 发现顺序 = `instances` 顺序 = codegen 发射顺序。`AOXN_TC_TRACE=1` 能直接看到这条链：

```text
[tc] main
[tc] id.i
[tc] id.f
[tc] first.i.?.3
[tc] first.f.?.2
[tc] unused_param.?
```

第一行是趟 3 的具体函数，后面全是趟 4 排空的实例。

#### 4.5 两条硬约束

1. **泛型声明绝不进入 codegen。** `check` 返回后，`src/lib.rs` 先
   `program.funcs.retain(|f| f.type_params.is_empty())` 丢掉泛型声明，再把 `out.instances` 追加进去；
   `codegen.rs` 遇到 `!f.type_params.is_empty()` 也直接 `continue`。原因很硬：`ty_of` 对数组长度是
   `LLVMArrayType(elem, len as u32)`，哨兵一进去就是 `[4294967295 x T]`（约 43 亿个元素），LLVM 会在
   这里炸掉。
2. **被检查的实例与被打发的实例必须是同一批对象（不得 clone）。** 趟 4 把队列里的 `FnDecl` **移动**进
   `instances`，`lib.rs` 再把它 `extend` 进 `program.funcs`；而 `call_map` 的键是**调用表达式在 AST 里的
   地址**。中间任何一步克隆了 AST，实例体内部的泛型调用就查不到路由（codegen 会退回按源码名字调用，
   于是调用一个根本不存在的 `@pick`）。`nested_generic_calls` 就是保护这条不变量的测试。

#### 4.6 例子

正确方向（一个声明、多份实例，长度参数当编译期常量用）：

```aoxn
# 每个不同的 (T, N) 在编译期生成一份具体实例
def first[T, N](arr: [T; N]) -> T:
    return arr[0]

def last[T, N](arr: [T; N]) -> T:
    return arr[N - 1]           # N 在实例里就是编译期 int

def main() -> int:
    print(first([10, 20, 30]))      # T=int,    N=3
    print(first([1.5, 2.5]))        # T=float,  N=2
    print(first(["a", "b", "c"]))   # T=string, N=3
    print(last([4, 5, 6, 7]))       # T=int,    N=4
    return 0
```

```text
10
1.500000
a
7
```

错误方向（都在编译期拒绝）：

```aoxn
def pair[T](a: T, b: T) -> T:
    return a

def main() -> int:
    print(pair(1, True))    # 编译期报错：argument 2 of 'pair' must be T, found bool
    return 0
```

```aoxn
# 长度参数只出现在返回类型里 -> 调用点推不出来
def make[T, N]() -> [T; N]:
    return [0]

def main() -> int:
    print(make())   # 编译期报错：cannot infer array length N (use it in a parameter)
    return 0
```

```aoxn
# 泛型体按"每个实例"单独检查：结构体没有 < 比较
def min_of[T, N](arr: [T; N]) -> T:
    m = arr[0]
    for x in arr:
        if x < m:
            m = x
    return m

struct P:
    v: int

def main() -> int:
    m = min_of([P(v=1), P(v=2)])
    print(m.v)      # 编译期报错：'<' requires two int, two float, or two string operands, found (P, P)
    return 0
```

最后一个例子值得记住：错误报在**泛型体内部**（`if x < m` 那一行），因为 `T` 上的操作是按实例检查的——
int/float/string 实例都过，结构体实例过不去。同一个程序如果把结果直接塞进
`print(min_of([...]))`，趟 3 会先报 `print requires int, float, bool, or string, found P`：调用点的用法
先于实例体检查。

#### 4.7 调试

```powershell
$env:AOXN_TC_TRACE = "1"     # 每个被检查的函数打一行 [tc] <名字>（含修饰后的实例名）
aoxn ir app.ax               # 直接看实例名、签名与函数体
```

`aoxn ir` 是确认"泛型是否真的变成了具体函数"的最快方式：泛型声明不会出现在 IR 里，只有修饰名的实例。

### 5. 与 codegen 的契约

`check` 返回 `CheckOutput { structs, sigs, instances, call_map }`（`src/typecheck.rs:70`）。真正交给
codegen 的是两样东西（`src/lib.rs` 的 `finish_to_object` / `finish_to_ir`，两个入口完全一致）：

```text
program.funcs.retain(|f| f.type_params.is_empty())     # 丢掉泛型声明
program.funcs.extend(out.instances)                    # 追加单态化实例（移动，不克隆）
codegen::generate_to_object(&program, obj, opt_level, &out.call_map)
```

- **类型表**：`structs` / `sigs` 是检查器自己的视图；codegen 不消费它们，而是从 `program.structs` /
  `program.funcs` 重建 `structs`（`LLVMStructCreateNamed` + `LLVMStructSetBody` 两阶段，允许互相引用）
  与 `struct_fields` / `fns`。检查器真正保证的是"到这一步的 `program` 里没有泛型声明、也没有
  `GENERIC_LEN` 残留"。
- **实例表**：`instances` 里每个 `FnDecl` 都是普通具体函数（`name` 已是修饰名、`type_params` 为空、
  参数/返回/函数体全部替换完毕），与手写的具体函数走完全一样的发射路径。
- **`call_map: HashMap<usize, String>`**：键是**调用表达式的 AST 节点地址**（`call_expr as *const Expr as usize`），
  值是实例名。codegen 在 `emit_expr` 的 `Expr::Call` 分支用同一个地址查表
  （`expr as *const Expr as usize`）：查到就用实例名，查不到就用源码里的名字（具体函数走这条路）。
- **为什么按 AST 节点地址索引**：修饰名是检查期才知道的信息，语法上没有对应写法，所以必须在"这个调用点"
  与"那个实例"之间建一条旁路映射；而 `Program` 从解析到发射始终是同一批对象（没有任何一阶段克隆它），
  节点地址天然就是稳定的站点标识。这也是仓库里既有的手法——字面量临时 alloca、聚合临时 alloca 同样按
  节点地址缓存，`import_aggregate_literals_across_files` 就是"按 id 而不是按地址缓存"踩坑后的回归测试。
  代价是这条不变量必须守住：检查与发射之间克隆 AST，路由就会断。

### 6. 规则 → 保护它的测试

`tests/pipeline.rs`（97 个端到端测试）里与本页直接相关的用例：

| 规则 | 测试 |
|---|---|
| 无隐式 int/float 转换 | `rejects_int_float_mixing` |
| 条件必须 bool | `rejects_non_bool_condition` |
| 未定义变量 | `rejects_undefined_variable` |
| 实参类型必须匹配 | `rejects_wrong_arg_type` |
| 所有路径返回值 | `rejects_missing_return` |
| 不可达代码 | `rejects_unreachable_code` |
| 重声明不能换类型 | `rejects_type_change_on_rebind` |
| 重赋值不能换类型 | `rejects_assign_wrong_type`（正向：`annotated_binding_reassign`） |
| 必须有 `main` | `rejects_missing_main` |
| `print` 拒绝聚合 | `rejects_print_struct` |
| 聚合不可比较 | `rejects_compare_compound` |
| 结构体未知字段 | `rejects_unknown_struct_field` |
| 结构体缺字段 | `rejects_missing_struct_field` |
| 结构体不能自包含 | `rejects_recursive_struct` |
| 只能索引数组 | `rejects_index_non_array` |
| 数组长度必须为正 | `rejects_zero_len_array`（parse 阶段） |
| 函数只收位置参数 | `rejects_struct_kwarg_on_fn` |
| 字符串拼接类型 | `rejects_concat_string_int` |
| 字符串比较类型 | `rejects_compare_string_int` |
| `break`/`continue` 必须在循环内 | `rejects_break_outside_loop`、`rejects_continue_outside_loop` |
| `range` 实参必须 int | `rejects_range_float_arg` |
| `for` 只能遍历数组 | `rejects_for_over_int` |
| f-string / `str()` 只接受可打印类型 | `rejects_fstring_of_array` |
| 泛型体按实例检查（结构体没有 `<`） | `rejects_generic_struct_sort` |
| 泛型不能是 `main` | `rejects_generic_main` |
| `main` 不能 extern | `rejects_extern_main` |
| 长度参数只允许出现在泛型函数内 | `rejects_len_param_outside_generic` |
| 长度参数必须可推断 | `rejects_uninferable_len` |
| 实例体内的嵌套泛型调用 | `nested_generic_calls` |
| 多组 `(T, N)` 共存 | `stdlib_generic_sort_search` |
| 单态化不破坏值语义 | `stdlib_sort_does_not_mutate` |
| 数组参数与返回类型 | `array_types_and_params` |
| 结构体前向引用与嵌套 | `struct_nested_and_forward_reference` |
| 数组/结构体值语义 | `array_value_semantics`、`struct_value_semantics` |
| 类型诊断落在正确的文件 | `import_error_reports_importing_file` |
| 单态化命名与自举实现一致 | `selfhost_typechecker_accepts_and_rejects` |
| 整个标准库能通过检查 | `selfhost_frontend_handles_stdlib` |

### 7. 检查器不做什么 / 已知边界

| 事项 | 实测行为 |
|---|---|
| 未使用的局部变量、参数、函数 | 不报错（检查器里没有"未使用"分析） |
| 从未被调用的泛型函数 | 函数体**完全不检查**：`def bad[T](x: T) -> int: return x + 1` 不被调用就能编译通过；具体函数则无条件检查 |
| 数组越界 | 不检查（C 风格未定义行为）；`arr[N]` 这种写法一样放行 |
| 丢弃非 void 返回值 | 允许（语句位置的普通调用，没有 must-use 规则） |
| `main` 的返回类型 | 这一层不管（`def main() -> string` 不会被拒） |
| `let` 注解里的未知类型名 | 不报 `unknown type`，而是当成一个不匹配的类型参与比较：`cannot initialize 'x: Foo' with an expression of type int`；只有函数参数/返回、结构体字段这些"被显式解析的位置"才报 `unknown type 'Foo'` |
| `extern` 用 `string` | 检查器不拦（`docs/spec.md` 写"暂不支持"，实现里 `string` 对 extern 就是 `ptr`） |
| 自包含检查的范围 | DFS 只沿**直接的 struct 类型字段**走：`struct A: xs: [A; 2]` 不会被报成 recursive，还能编译出可执行文件 |
| `[e] * 0` / `[0] * n` / `[1, 2] * 3` | 解析器只把"单元素字面量 × 正整数字面量"当复制，其余落回普通乘法：报 `'*' requires two int or two float operands, found ([int; 1], int)` |
| 只写在头部的长度参数 | 体内引用它会变成未定义名：`def f[N]() -> int: return N` → `unknown variable 'N'`（它只在数组长度位置参与替换） |
| 诊断原文的权威来源 | `docs/spec.md` 仍标 v0.9（例如 Statements 一节还写着 "no `for` yet"），**以 `src/typecheck.rs` + `tests/pipeline.rs` 为准** |

一个内部细节：`Expr::StructLit` 这个 AST 节点在检查器与 codegen 里都有分支，但当前解析器不产生它——
结构体构造走的是"具名实参的 `Expr::Call`"。所以重复字段的真实诊断是
`field 'x' given more than once in 'P'`，而不是 `StructLit` 分支里那句 `... in struct literal`。

### 8. 延伸阅读

- 缩进、`[T; N]` 与泛型头部的解析 → [词法与语法](Frontend-Lexer-and-Parser.md)
- 类型检查之后：ABI、聚合地址、IR → [代码生成与 LLVM](Codegen-and-LLVM-FFI.md)
- 编译器整体流水线 → [编译器架构](Compiler-Architecture.md)
- 用泛型写成的标准库 → [标准库](Standard-Library.md)
- 命令行、优化级别与环境变量完整表 → [命令行与工具链](CLI-and-Tooling.md)
- 测试组织与 CI → [测试与 CI](Testing-and-CI.md)
- 自举的 Aoxn 版检查器 → [自举](Self-Hosting.md)

## English

### 0. Quick self-check: what gets rejected

`typecheck.rs` takes the position that anything statically decidable is decided now: no implicit
conversions, no overload resolution, no lenient data-flow guessing. The most common rejections
(every one of them maps to a rule and a verbatim diagnostic in §3):

| What you wrote | What you get |
|---|---|
| `x: int = 1 + 1.5` | `'+' requires two int or two float operands, found (int, float)` |
| `if 1:` / `while 1:` | conditions must be `bool` |
| A non-`void` function with a path that does not `return` | `... does not return a value on all paths` |
| A statement after `return` | `unreachable statement after 'return'` |
| Binding the same name to another type | `cannot re-declare ...` / `cannot assign a value of type ...` |
| `[]` or `[1, 2.5]` | empty array literal / mixed element types |
| A struct literal missing a field, repeating one, or naming a bad one | `missing field(s)` / `given more than once` / `has no field` |
| `print(array or struct)` | `print requires int, float, bool, or string` |
| `len(3)` | `len requires an array or string` |
| `def main[T]` or `extern def main` | neither generics nor extern may be `main` |
| `[int; N]` in a non-generic signature | the parser reports `unknown array length 'N'` |
| A generic length parameter no argument can pin down | `cannot infer array length N` |

Two easy misjudgements: **unused names (locals, parameters, functions) are not diagnosed**, and
**a generic function that is never called has its body checked never** (see §7).

### 1. The five passes

The order inside `check(&Program)` (`src/typecheck.rs:77`):

| Pass | What it does | Why it matters |
|---|---|---|
| 0 | `collect_structs` builds the `StructTable` | reserve all names first (forward references) → resolve field types → DFS for self-containment |
| 1 | concrete function signatures | all `FnSig`s land first, so recursion and mutual recursion do not depend on declaration order; parameters, return types, `void` parameters and duplicate names are checked here |
| 2 | generic declarations are registered | the `FnDecl` is stored, its **body is not checked** (that waits for instantiation) |
| 3 | bodies of concrete (non-extern, non-generic) functions | declaration order; each function sets `cur_file`, so diagnostics carry the right file |
| 4 | drain the instance queue | FIFO; instance bodies may enqueue further instances (nested generics); every instance **is** the object codegen will emit |

`main`'s existence is checked after pass 2 and before pass 3 (`program has no 'main' function`), so a
missing `main` masks type errors inside function bodies.

### 2. Type environment and symbol tables

- **`Type`** (`src/ast.rs`): `int | float | bool | string | void | [T; N] | Name`. Equality is
  structural; there are no subtypes, no aliases, no implicit conversions.
- **`FnSig { params: Vec<Type>, ret: Type }`**: concrete functions only, keyed by name in `sigs` (the
  `FastBuild` hasher). Generic declarations live in `generics`; the two maps are disjoint, so one name
  can never be in both.
- **`StructTable = HashMap<String, StructInfo>`**; since v0.26.3 `StructInfo { fields: Vec<(String, Type)>, index }`
  and `field_type()` goes through `index` → O(1) field lookup (the old linear scan was measurable on
  40-field state structs).
- **Structs and functions share one namespace**: a call `P(x=1)` resolves through `sigs` → `generics`
  → `structs`; hitting a struct means "named-field construction", and only when all three miss do you
  get `call to undefined function or struct 'P'`.
- **Scopes**: one **flat** `HashMap<String, Type>` per function, pre-seeded with the parameters;
  `if`/`while`/`for` blocks neither push nor pop. Bindings are therefore function-scoped
  (Python-like): a `let` inside a block stays visible after it (an `if True: z = 1` followed by
  `print(z)` compiles), and there is **no shadowing** — a second binding of the same name is a
  re-assignment whose type must match.
- **Diagnostic file**: `Tc::cur_file` follows the function being checked; the `file` field of
  `Diag::at("type", file, line, col, msg)` indexes the `src/files.rs` registry and is resolved to a
  path only when printing/JSON-encoding. In a multi-file build the error therefore points at the file
  that really failed (`import_error_reports_importing_file` protects this).

### 3. The strict semantic rules

#### 3.1 Operators and expressions

| Rule | Diagnostic on violation (verbatim) |
|---|---|
| `+ - * /` take two `int`s or two `float`s | `'+' requires two int or two float operands, found (int, float)` (`-`/`*`/`/` use the same shape) |
| `%` takes two `int`s | `'%' requires two int operands, found (float, float)` |
| `+` on strings is concatenation, string + string only | `cannot concatenate string with int` |
| `==`/`!=` need identical, non-void operand types | `cannot compare float with int` |
| Aggregates (arrays/structs) are not comparable | `cannot compare compound type P with P` |
| `< <= > >=` take (int,int)/(float,float)/(string,string) | `'<' requires two int, two float, or two string operands, found (P, P)` |
| `and`/`or` (`&&`/`||`) need `bool` operands | `'&&' requires bool operands, found (int, int)` |
| `not` (`!`) takes a `bool` | `'!' requires bool, found int` |
| Unary `-` takes an `int` or `float` | `unary '-' requires int or float, found string` |
| An index must be an `int` | `array index must be int, found float` |
| Only arrays are indexable (not strings/structs/ints) | in expression position: `cannot index a value of type string (only [T; N] arrays are indexable)`; as an assignment target the same message stops after the type |

Note that `op_str` prints `and`/`or` as `&&`/`||` too: source `1 and 2` is reported as
`'&&' requires bool operands`.

#### 3.2 Conditions and loops

| Rule | Diagnostic on violation |
|---|---|
| `if` conditions must be `bool` | `'if' condition must be bool, found int` |
| `while` conditions must be `bool` | `'while' condition must be bool, found int` |
| `range` takes 1..3 `int` arguments | `range expects 1 to 3 arguments, found 0` / `range arguments must be int, found float` |
| `for x in ...` iterates arrays only (not even strings) | `'for' can only iterate over arrays, found string` |
| `break`/`continue` need a loop | `'break' outside of a loop` / `'continue' outside of a loop` |
| The loop variable follows normal binding rules | `loop variable 'x' is already int, cannot reuse as float` |

#### 3.3 Returns and reachability

| Rule | Diagnostic on violation |
|---|---|
| A non-void function returns on every path | `function 'f' returns int but does not return a value on all paths` |
| A bare `return` is invalid when a value is due | `'return' must return a value of type int` |
| A void function cannot return a value | `void function cannot return a value` |
| The returned type must match exactly | `'return' type mismatch: expected int, found float` |
| Nothing may follow a `return` in the same block | `unreachable statement after 'return'` |

The "guaranteed return" analysis is deliberately conservative and recognises exactly two statements
(`stmt_guarantees_return`): `return`, and an `if` where **both** `then` and `else` guarantee a return
(the `elif` chain is folded into nested `if`s at parse time, so an exhaustive `if/elif/else` counts).
**`while True:` does not count** — this is rejected:

```aoxn
def f() -> int:
    while True:
        return 1        # compile error: function 'f' returns int but does not return a value on all paths

def main() -> int:
    print(f())
    return 0
```

The checker looks at statement shape only; it does not reason about whether a loop terminates.

The unreachable check is **forward within a block**: once a statement guarantees a return, any later
statement in that block is an error; code after an `if/elif/else` whose branches all return is
rejected the same way (verified: `unreachable statement after 'return'`).

#### 3.4 Bindings and assignment

| Rule | Diagnostic on violation |
|---|---|
| First binding: the annotation must match the initializer | `cannot initialize 'x: int' with an expression of type float` |
| A void expression cannot be bound | `cannot bind a void expression to 'x'` |
| Re-assignment/re-declaration: the type is fixed | with an annotation: `cannot re-declare 'x' as float: it is already int`; without one: `cannot assign a value of type string to 'x: int'` |
| Compound targets (index/field) keep their type | `cannot assign a value of type float to a target of type int` |
| The target must be a variable, index or field | rejected in the parser: `invalid assignment target` |
| No assignment to an undeclared name | a compound target reports `unknown variable 'y'` first (a bare `y = ...` is a binding, not an assignment) |

Parameters join the same table: re-typing a parameter inside the body is an ordinary re-assignment
error.

#### 3.5 Arrays

| Rule | Diagnostic on violation |
|---|---|
| Literals must be non-empty | parse stage: `empty array literals are not allowed (element type could not be inferred)` |
| All elements share one type (the first element decides) | `array literal elements must share one type: found int and float` |
| A length must be a positive integer literal | parse stage: `array length must be a positive integer` |
| A replication count must be positive | the checker has `array replication count must be positive`, but the parser only builds a replication node for "single-element literal × positive integer literal" (see §7) |
| Indices are `int`s, only arrays are indexable | see 3.1 |

`len(x)` returns the **static** length as an `int`. Array indexing is unchecked (C-style;
out-of-bounds is undefined behaviour).

#### 3.6 Structs

| Rule | Diagnostic on violation |
|---|---|
| Field names are unique | `duplicate field 'x' in struct 'P'` |
| Field types must exist | `unknown type 'Foo'` |
| A field cannot be `void` | `field 'x' cannot have type void` |
| A struct cannot contain itself | `recursive struct 'Node' (a struct cannot contain itself, directly or indirectly)` |
| Construction is always by name | `struct 'P' must be constructed with named fields: P(field=value, ...)` |
| Every field exactly once, names must exist, types must match | missing: `struct literal 'P' is missing field(s): y`; repeated: `field 'x' given more than once in 'P'`; unknown: `struct 'P' has no field 'z'`; wrong type: `field 'x' of 'P' must be int, found float` |
| Field access must exist | `type P has no field 'z'` |
| Struct names are unique | `struct 'P' is defined more than once`; colliding with a function reports `'P' is already defined as a struct` for whichever comes second |

Field order does not matter (construction matches by name); structs may forward-reference each other
(names are reserved before field types are resolved).

#### 3.7 Builtins

| Rule | Diagnostic on violation |
|---|---|
| `print` takes one positional argument of int/float/bool/string | `print expects exactly 1 positional argument` / `print requires int, float, bool, or string, found [int; 2]` |
| `len` takes one positional argument, an array or string | `len expects exactly 1 positional argument` / `len requires an array or string, found int` |
| `str` takes one positional printable argument | `str expects exactly 1 positional argument` / `cannot convert P to string` (f-strings desugar to `str()`) |
| Named arguments belong to struct construction only | `function 'id' takes positional arguments only` |

The other builtins (`load_i64`, `store_u8`, `as_ptr`, `as_string`, `target_os`, …) are checked in the
same layer: their signatures and messages are written out one by one in the `Expr::Call` arm of
`check_expr`, not looked up in a language-level function table.

#### 3.8 Names and definitions

| Rule | Diagnostic on violation |
|---|---|
| Functions are unique (generics and concrete share the namespace) | `function 'f' is defined more than once` |
| Parameters are unique | `duplicate parameter 'x' in function 'f'` |
| Parameters need a type and cannot be `void` | `parameter 'x' cannot have type void` |
| Type names must exist (where they are resolved) | `unknown type 'Foo'` |
| `main` must exist | `program has no 'main' function` |
| `main` cannot be extern | `'main' cannot be declared extern` |
| Every used name must be bound | `unknown variable 'x'` |
| Every call must resolve | `call to undefined function or struct 'g'` |
| Argument count and types | `function 'f' expects 1 argument(s), found 2` / `argument 1 of 'f' must be int, found float` |

### 4. Generic monomorphization

Aoxn generics have no runtime representation: every distinct `(T, N)` combination produces a dedicated
concrete instance at compile time, and codegen never sees the generic declaration itself.

#### 4.1 Syntax and the sentinel

```aoxn
def sort[T, N](arr: [T; N]) -> [T; N]:
    result = arr
    return result
```

(The block above is a fragment, showing the header and body shape only; complete runnable programs
are in §4.6.)

- Every name in the header `[T, N]` becomes a `type_params` entry; a type parameter used in an array
  length slot lands in `len_param` (**the first one only**; a second one makes the parser report
  `only one length parameter is supported (found 'M' as well)`).
- The `N` in `[T; N]` parses to the **`GENERIC_LEN = usize::MAX`** sentinel (`src/ast.rs:63`).
  `Type`'s `Display` prints it back as `[T; N]`, so diagnostics keep the source shape.
- Writing `[int; N]` in a **non-generic** function is a parse-stage error:
  `unknown array length 'N' (length parameters must be declared in the fn header, e.g. def f[T, N](arr: [T; N]))`.
  The checker keeps a second line of defence against the sentinel:
  `array length parameters are only valid inside generic functions`.
- A generic function cannot be `main` (`'main' cannot be generic`) and cannot be `extern`
  (`extern functions cannot be generic`).

#### 4.2 What one call site does (`instantiate_call`)

1. **Check the arguments** and collect their types; a named argument is rejected with
   `takes positional arguments only`, a wrong count with
   `function '{name}' expects {n} argument(s), found {m}`.
2. **Unify** the declared parameter types (possibly `T`, `[T; N]`) against the argument types:
   - a `T` binds into `subst_t`; a second occurrence in the same call must agree with the first
     (this is where `pair(1, True)` dies);
   - an `[.. ; N]` binds the length `n`; a second occurrence must agree;
   - a fixed-length array requires equal lengths and then recurses into the element type; everything
     else is **exact equality**;
   - any failure reports `argument {i} of '{name}' must be {declared parameter type}, found {actual}`.
3. **Substitute the return type**: `subst_type(ret)`; if the return type still needs an `N` that no
   argument supplied → `cannot infer array length N (use it in a parameter)`.
4. **Name it and record the route**: `mangle(...)` produces the instance name, then
   `call_map.insert(address of the call expression node, instance name)`.
5. **Enqueue an instance the first time it is seen**: clone the generic declaration → rename it →
   clear `type_params` → substitute parameter types, the return type and the **entire body**
   (`subst_block_types`, which rewrites `let` annotations and every `T`/`N` inside expressions). An
   `N` in the body becomes an `Expr::Int(length)` with a synthetic position (`file = u32::MAX`), so
   inside an instance the length is a compile-time `int` constant: `range(N)`, `N - 1`, `arr[N - 1]`
   all work.

Pass 4 then checks the queued instance bodies under **exactly the same rules**, so a generic call
inside an instance body enqueues further instances until the queue drains.

#### 4.3 Instance naming

`mangle` (`src/typecheck.rs:1003`) is `name` + one `.slug` per declared type parameter + (when a length
was bound) `.length`:

| Type | Slug |
|---|---|
| int / float / bool / string / void | `i` / `f` / `b` / `s` / `v` |
| `[E; L]` | `a<element slug>x<L>` |
| struct `P` | `s_P` |
| a type parameter nothing constrained | `?` |

Measured (function names printed by `aoxn ir`):

```text
id(1), id(2)              -> id.i              two calls of the same shape share one instance
id(3.5), id(4.5)          -> id.f
first([10, 20, 30])       -> first.i.?.3       T=int; the length parameter sits in the type slot as ".?" and the bound length is appended
first([1.5, 2.5])         -> first.f.?.2
first(["a", "b", "c"])    -> first.s.?.3
last([4, 5, 6, 7])        -> last.i.?.4
sum_int([1, 2, 3])        -> sum_int.?.3       only a length parameter N, no T
unused_param[T](x: int)   -> unused_param.?
```

The `.?` in `first.i.?.3` is worth explaining: the length parameter `N` also occupies a slot in
`type_params`, but it never takes part in type substitution, so its slot is `?` and the real length is
appended last. The Aoxn-written checker uses the same scheme: the output of `selfhost/tycheck_demo.ax`
is asserted verbatim by `selfhost_typechecker_accepts_and_rejects`, including `wrap.i`, `wrap.b`,
`id.i`, `id.b` and `first.i.?.3`.

#### 4.4 Deduplication and order

- `done: HashSet<String>` deduplicates by **mangled name**: one `(T, N)` is cloned, checked and emitted
  once, and every call site shares it.
- `queue: VecDeque<FnDecl>` is FIFO (v0.26.3 replaced `Vec::remove(0)`, which was O(m²) in the instance
  count).
- Queue order = discovery order = `instances` order = emission order. `AOXN_TC_TRACE=1` shows the
  whole chain:

```text
[tc] main
[tc] id.i
[tc] id.f
[tc] first.i.?.3
[tc] first.f.?.2
[tc] unused_param.?
```

The first line is a pass-3 concrete function; everything after it is a pass-4 instance.

#### 4.5 Two hard constraints

1. **Generic declarations never reach codegen.** After `check` returns, `src/lib.rs` first drops them
   with `program.funcs.retain(|f| f.type_params.is_empty())` and then appends `out.instances`;
   `codegen.rs` also `continue`s on any `!f.type_params.is_empty()`. The reason is unforgiving:
   `ty_of` lowers an array length as `LLVMArrayType(elem, len as u32)`, so the sentinel becomes
   `[4294967295 x T]` (some 4.3 billion elements) and LLVM dies there.
2. **The checked instances and the emitted instances must be the same objects (never clone).** Pass 4
   **moves** the queued `FnDecl`s into `instances` and `lib.rs` extends `program.funcs` with them, while
   `call_map` is keyed by the **AST address of the call expression**. Cloning the AST anywhere in
   between loses the routes of the generic calls inside instance bodies (codegen falls back to the
   source name and then calls a `@pick` that does not exist). `nested_generic_calls` is the test that
   protects this invariant.

#### 4.6 Examples

The correct direction (one declaration, several instances, the length parameter used as a constant):

```aoxn
# every distinct (T, N) gets a concrete instance at compile time
def first[T, N](arr: [T; N]) -> T:
    return arr[0]

def last[T, N](arr: [T; N]) -> T:
    return arr[N - 1]           # N is a compile-time int inside the instance

def main() -> int:
    print(first([10, 20, 30]))      # T=int,    N=3
    print(first([1.5, 2.5]))        # T=float,  N=2
    print(first(["a", "b", "c"]))   # T=string, N=3
    print(last([4, 5, 6, 7]))       # T=int,    N=4
    return 0
```

```text
10
1.500000
a
7
```

The wrong direction (all rejected at compile time):

```aoxn
def pair[T](a: T, b: T) -> T:
    return a

def main() -> int:
    print(pair(1, True))    # compile error: argument 2 of 'pair' must be T, found bool
    return 0
```

```aoxn
# the length parameter only appears in the return type -> no call site can infer it
def make[T, N]() -> [T; N]:
    return [0]

def main() -> int:
    print(make())   # compile error: cannot infer array length N (use it in a parameter)
    return 0
```

```aoxn
# a generic body is checked per instance: structs have no < comparison
def min_of[T, N](arr: [T; N]) -> T:
    m = arr[0]
    for x in arr:
        if x < m:
            m = x
    return m

struct P:
    v: int

def main() -> int:
    m = min_of([P(v=1), P(v=2)])
    print(m.v)      # compile error: '<' requires two int, two float, or two string operands, found (P, P)
    return 0
```

The last one is worth remembering: the error points **inside the generic body** (the `if x < m` line)
because operations on `T` are checked per instance — int/float/string instances all pass, a struct
instance does not. Had the program fed the result straight into `print(min_of([...]))`, pass 3 would
have reported `print requires int, float, bool, or string, found P` first: how the call site uses the
result is checked before the instance body.

#### 4.7 Debugging

```powershell
$env:AOXN_TC_TRACE = "1"     # one [tc] <name> line per checked function (mangled instance names included)
aoxn ir app.ax               # instance names, signatures and bodies, straight from the IR
```

`aoxn ir` is the fastest way to confirm that a generic really became concrete functions: the generic
declaration never appears in the IR, only the mangled instances do.

### 5. The contract with codegen

`check` returns `CheckOutput { structs, sigs, instances, call_map }` (`src/typecheck.rs:70`). What
actually reaches codegen are two things (`finish_to_object` / `finish_to_ir` in `src/lib.rs`, identical
in both entry points):

```text
program.funcs.retain(|f| f.type_params.is_empty())     # drop the generic declarations
program.funcs.extend(out.instances)                    # append the monomorphized instances (move, no clone)
codegen::generate_to_object(&program, obj, opt_level, &out.call_map)
```

- **Type tables**: `structs` / `sigs` are the checker's own view; codegen does not consume them. It
  rebuilds `structs` (in two phases, `LLVMStructCreateNamed` + `LLVMStructSetBody`, so structs may
  reference each other) plus `struct_fields` / `fns` from `program.structs` / `program.funcs`. What the
  checker really guarantees is that the `program` reaching this point holds no generic declarations and
  no leftover `GENERIC_LEN`.
- **Instances**: every `FnDecl` in `instances` is an ordinary concrete function (its `name` is already
  mangled, `type_params` is empty, and parameters/return/body are fully substituted), emitted through
  exactly the same path as a hand-written concrete function.
- **`call_map: HashMap<usize, String>`**: keys are **AST node addresses of call expressions**
  (`call_expr as *const Expr as usize`), values are instance names. Codegen looks up the same address in
  the `Expr::Call` arm of `emit_expr` (`expr as *const Expr as usize`): a hit selects the instance name,
  a miss keeps the syntactic name (the path concrete functions take).
- **Why index by AST node address**: the mangled name is knowledge that only exists during checking and
  has no syntax of its own, so a side channel between "this call site" and "that instance" is
  unavoidable; and `Program` stays one set of objects from parsing to emission (no stage clones it), so
  a node address is naturally a stable site identity. This is also an established pattern in the repo —
  literal and aggregate temporary allocas are cached by node address too, and
  `import_aggregate_literals_across_files` is the regression test left behind by caching by id instead.
  The price is that the invariant must hold: cloning the AST between checking and emission breaks the
  routes.

### 6. Rule → protecting test

The cases in `tests/pipeline.rs` (97 end-to-end tests) that speak directly to this page:

| Rule | Test |
|---|---|
| No implicit int/float conversions | `rejects_int_float_mixing` |
| Conditions must be bool | `rejects_non_bool_condition` |
| Undefined variable | `rejects_undefined_variable` |
| Argument types must match | `rejects_wrong_arg_type` |
| All paths return | `rejects_missing_return` |
| Unreachable code | `rejects_unreachable_code` |
| Re-declaration cannot change the type | `rejects_type_change_on_rebind` |
| Re-assignment cannot change the type | `rejects_assign_wrong_type` (positive: `annotated_binding_reassign`) |
| `main` must exist | `rejects_missing_main` |
| `print` rejects aggregates | `rejects_print_struct` |
| Aggregates are not comparable | `rejects_compare_compound` |
| Unknown struct field | `rejects_unknown_struct_field` |
| Missing struct field | `rejects_missing_struct_field` |
| A struct cannot contain itself | `rejects_recursive_struct` |
| Only arrays are indexable | `rejects_index_non_array` |
| Array lengths must be positive | `rejects_zero_len_array` (parse stage) |
| Functions take positional arguments only | `rejects_struct_kwarg_on_fn` |
| String concatenation types | `rejects_concat_string_int` |
| String comparison types | `rejects_compare_string_int` |
| `break`/`continue` need a loop | `rejects_break_outside_loop`, `rejects_continue_outside_loop` |
| `range` arguments must be int | `rejects_range_float_arg` |
| `for` iterates arrays only | `rejects_for_over_int` |
| f-strings / `str()` accept printable types only | `rejects_fstring_of_array` |
| Generic bodies are checked per instance (structs lack `<`) | `rejects_generic_struct_sort` |
| A generic cannot be `main` | `rejects_generic_main` |
| `main` cannot be extern | `rejects_extern_main` |
| Length parameters only inside generic functions | `rejects_len_param_outside_generic` |
| Length parameters must be inferable | `rejects_uninferable_len` |
| Generic calls inside instance bodies | `nested_generic_calls` |
| Several `(T, N)` combinations side by side | `stdlib_generic_sort_search` |
| Monomorphization preserves value semantics | `stdlib_sort_does_not_mutate` |
| Array parameters and returns | `array_types_and_params` |
| Struct forward references and nesting | `struct_nested_and_forward_reference` |
| Array/struct value semantics | `array_value_semantics`, `struct_value_semantics` |
| Type diagnostics land in the right file | `import_error_reports_importing_file` |
| The mangling matches the self-hosted checker | `selfhost_typechecker_accepts_and_rejects` |
| The whole standard library typechecks | `selfhost_frontend_handles_stdlib` |

### 7. What the checker does not do / known edges

| Subject | Observed behaviour |
|---|---|
| Unused locals, parameters, functions | no diagnostic (there is no unused-name analysis in the checker) |
| A generic function that is never called | its body is **not checked at all**: `def bad[T](x: T) -> int: return x + 1` compiles as long as nothing calls it; concrete function bodies are always checked |
| Array bounds | not checked (C-style undefined behaviour); `arr[N]` is let through like any other index |
| Discarding a non-void result | allowed (a plain call in statement position; there is no must-use rule) |
| `main`'s return type | not constrained at this layer (`def main() -> string` is not rejected) |
| An unknown type name in a `let` annotation | not reported as `unknown type`; it takes part in the comparison as an opaque name: `cannot initialize 'x: Foo' with an expression of type int`. Only the positions that are explicitly resolved (function parameters/returns, struct fields) report `unknown type 'Foo'` |
| `extern` with `string` | not blocked by the checker (`docs/spec.md` calls it unsupported; in the implementation `string` is just `ptr` for an extern) |
| Scope of the self-containment check | the DFS only follows **direct struct-typed fields**: `struct A: xs: [A; 2]` is not reported as recursive and still builds an executable |
| `[e] * 0` / `[0] * n` / `[1, 2] * 3` | the parser only treats "single-element literal × positive integer literal" as replication; everything else falls back to multiplication and reports `'*' requires two int or two float operands, found ([int; 1], int)` |
| A length parameter that only appears in the header | referring to it in the body is an undefined name: `def f[N]() -> int: return N` → `unknown variable 'N'` (it is substituted only in array length positions) |
| Authority for verbatim diagnostics | `docs/spec.md` is still labelled v0.9 (its Statements section even says "no `for` yet"); treat `src/typecheck.rs` + `tests/pipeline.rs` as the source of truth |

One internals detail: the `Expr::StructLit` AST node still has arms in both the checker and codegen,
but the current parser never produces it — struct construction is an `Expr::Call` with named arguments.
The live duplicate-field diagnostic is therefore `field 'x' given more than once in 'P'`, not the
`... in struct literal` wording of the `StructLit` arm.

### 8. Further reading

- Indentation, `[T; N]` and generic headers as parsed → [Lexer and Parser](Frontend-Lexer-and-Parser.md)
- What happens after checking: ABI, aggregate addresses, IR → [Codegen and LLVM](Codegen-and-LLVM-FFI.md)
- The compiler pipeline as a whole → [Compiler Architecture](Compiler-Architecture.md)
- The standard library that leans on generics → [Standard Library](Standard-Library.md)
- CLI, optimization levels and the full environment-variable table → [CLI and Tooling](CLI-and-Tooling.md)
- How the tests and CI are organised → [Testing and CI](Testing-and-CI.md)
- The Aoxn-written checker used for self-hosting → [Self-Hosting](Self-Hosting.md)

---

## 源文件 / Source files

- [src/typecheck.rs](../src/typecheck.rs) — the checker: `check`, `Tc`, `FnSig`, `StructInfo`, `unify`, `subst_type`, `mangle`, `collect_structs`
- [src/ast.rs](../src/ast.rs) — `Type`, `FnDecl` (`type_params`, `len_param`), `GENERIC_LEN`, `Expr::is_lvalue`, `Type::is_printable` / `is_compound`
- [src/parser.rs](../src/parser.rs) — generic headers, `[T; N]` → `GENERIC_LEN`, positive array lengths, empty literals, replication
- [src/lib.rs](../src/lib.rs) — `finish_to_object` / `finish_to_ir`: drop generic declarations, append instances, pass `call_map`
- [src/codegen.rs](../src/codegen.rs) — `ty_of` (`LLVMArrayType`), the skip of generic declarations, the `call_map` lookup in `emit_expr`
- [tests/pipeline.rs](../tests/pipeline.rs) — the 97-test end-to-end suite behind the rule → test table
- [stdlib/stdlib.ax](../stdlib/stdlib.ax) — the generic functions the tests exercise
- [selfhost/typecheck.ax](../selfhost/typecheck.ax) — the Aoxn-written checker (same mangling)
- [selfhost/tycheck_demo.ax](../selfhost/tycheck_demo.ax) — the demo whose instance names a test asserts
- [docs/spec.md](../docs/spec.md) — the language specification (still labelled v0.9; outdated in places)
- [CONTRIBUTING.md](../CONTRIBUTING.md) — the strict-semantics house rules and the aggregate-ABI note
- [CHANGELOG.md](../CHANGELOG.md) — v0.26.3 notes (the `VecDeque` queue, `StructInfo { fields, index }`)
