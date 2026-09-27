# 前端：词法与语法 · Frontend: Lexer and Parser

> **中文**：Aoxn 前端的两个阶段——布局式（Python 风格）词法分析如何产出带位置的 token 流，以及递归下降
> 解析器如何把它变成 `src/ast.rs` 的 AST，包括 f-string 的切分与脱糖、泛型头的处理与错误定位。
> **English**: The two front-end stages of Aoxn — how the layout-aware (Python-style) lexer produces a
> positioned token stream, and how the recursive-descent parser turns it into the `src/ast.rs` AST,
> including f-string splitting and desugaring, generic headers, and error locations.

## 中文

### 1. 位置：流水线的头两段

```text
 .ax 源码文本
      │  src/lexer.rs ::  lex(src: &str, file_id: u32) -> Result<Vec<Token>, Diag>
      ▼  Token { tok: Tok, pos: Pos{ line, col, file } }；含 Newline / Indent / Dedent
 token 流
      │  src/parser.rs :: parse(tokens: Vec<Token>) -> Result<Program, Diag>
      ▼  src/ast.rs 的 Program（递归下降、单 token 前瞻）
 AST
      │  → src/typecheck.rs → src/codegen.rs
```

前端只做两件事：把字符流变成 token 流，再把 token 流变成树。所有语义判断（类型、返回值、
未定义变量）都不在这里。两个阶段都返回 `Result<_, Diag>`，第一个错误就中止——**没有错误恢复，
一次编译只报一个诊断**。

### 2. 布局式词法

#### 2.1 缩进栈：Indent / Dedent 的产生规则

词法器持有一个 `indent_stack`，初始为 `vec![0]`。每次进入新行（且不在括号内）先测量行首缩进：

```text
 ' '  → indent = indent + 1
 '\t' → indent = (indent / 4 + 1) * 4      # 制表符对齐到下一个 4 列边界
 其它 → 停止测量
```

然后与栈顶比较：

- `indent > 栈顶`：压栈并产出一个 `Indent`（位置固定为「该行, col 1」）。
- `indent < 栈顶`：反复弹栈，每次弹栈产出一个 `Dedent`；弹完后若栈顶仍不等于 `indent`，报错：
  `unindent does not match any outer indentation level (expected one of […], found n)`。
- 相等：什么都不产出（普通语句行）。

只有缩进宽度本身参与比较，没有任何“上一行必须多 4 个空格”的规定，只要**同一层一致**即可。

#### 2.2 空行与纯注释行不产生任何 token

测量完缩进后看行首第一个字符：

- 是 `\n`（或 `\r\n`）：这是空行，直接跳过，**不产出 `Newline`，也不改变缩进栈**。
- 是 `#`：直接跳到行尾（行首空白在测量缩进时已经被吃掉），**同样不产出任何 token**。
- 是其它字符：先按 2.1 处理缩进，然后正常扫描该行的 token。

这一点很容易被忽略：文件开头的注释行不会产生 `Newline`，所以 `examples/hello.ax` 的 token 流从
`Def` 开始（见 §9 的实测输出）。

#### 2.3 括号内忽略换行与缩进（隐式续行）

`(` 和 `[` 让 `paren_depth` 加一，`)` 和 `]` 让它减一（两者不区分，用 `]` 关掉 `(` 也接受）。当
`paren_depth > 0` 时：

- `\n` 不产出 `Newline`，也不把 `at_line_start` 置真——于是下一行的缩进测量整段被跳过，行首空白
  只是普通空白。
- 结果是 `print(` 之后可以跨任意多行写实参、每个实参一行、末尾还能有逗号（`tests/pipeline.rs` 的
  `multiline_call_arguments` 就是这个行为）。

两个边界情况都有明确报错：

- 闭括号多于开括号（`paren_depth` 变负）：`unmatched closing ')'`。
- 到文件结束 `paren_depth` 仍大于 0：`unclosed bracket: '(' or '[' was never closed`，位置在 EOF
  （实测报在最后一行之后，例如 `4:1`）。

唯一的例外是 f-string 的插值子扫描：它用一个“虚拟括号”（`virtual_paren`）打开 `paren_depth = 1`，
因此不会在扫描结束时误报未闭合（见 §4.3）。

#### 2.4 EOF 收尾

扫描循环结束后按固定顺序补齐：

```text
 1. 若最后一个 token 不是 Newline → 补一个 Newline（位置 = 当前 line/col）
 2. 只要缩进栈深度 > 1 → 弹栈并各补一个 Dedent（同样用 EOF 位置）
 3. 补一个 Eof
```

所以任何合法的 token 流都以 `… Eof` 结束，且 `Eof` 之前一定是 `Dedent` 或 `Newline`。

#### 2.5 注释只有 `#`

`#` 在行首分支和主循环里各处理一次，效果都是“吃到行尾”，没有块注释、没有文档注释。

`//` **不是**注释符：词法里根本没有这条规则，`'/'` 只是一个单字符 token，所以 `17 // 5` 会得到两个
`Tok::Slash`。当前 parser 只把**单个** `Slash` 映射为 `BinOp::Div`，而 `int` 的 `/` 本身就是截断除法
（`17 / 5` 得 `3`，见 `tests/pipeline.rs` 的 `arithmetic_and_precedence`）。仓库文档
（`CONTRIBUTING.md`）把 `//` 描述成“整数除法”，但 v0.26.3 的实现没有 `//` 这个 token：写成
`17 // 5` 会报 `expected an expression, found Slash`。**除法写 `/`。**

### 3. Token 结构与位置

```text
 pub struct Token {
     pub tok: Tok,
     pub pos: Pos,          // { line, col, file }
 }

 pub struct Pos {
     pub line: usize,       // 从 1 开始
     pub col: usize,        // 从 1 开始；tab 按 4 列推进
     pub file: u32,         // 本次编译文件注册表的下标（src/files.rs）
 }

 pub enum FStrPart {
     Lit(String),                  // f-string 里的字面片段
     ExprTokens(Vec<Token>),       // 已经子词法化好的插值表达式 token
 }
```

`Pos` 在每个 token 开始扫描**之前**捕获，所以它就是 token 的起始位置；`Indent` 是唯一的例外，列号固定
写成 1。`file` 只是索引，人类可读名字在 `src/files.rs` 的注册表里，`diag_to_string` 打印或
`diags_to_json` 序列化时才去换（见 [编译器架构](Compiler-Architecture.md) §4）。

`Tok` 是单个扁平枚举（没有子结构体），关键字在这里就折叠成专用变体：

| 源码写法 | token |
|---|---|
| `def` `struct` `extern` `import` `if` `elif` `else` `while` `for` `in` | `Def` `Struct` `Extern` `Import` `If` `Elif` `Else` `While` `For` `In` |
| `break` `continue` `return` `pass` | `Break` `Continue` `Return` `Pass` |
| `and` `or` `not` | `AndAnd` `OrOr` `Bang`（**没有**独立关键字 token） |
| `true` `True` / `false` `False` | `True` / `False` |
| `int` `float` `bool` `string` `void` | `TyInt` `TyFloat` `TyBool` `TyString` `TyVoid` |
| 其它标识符 | `Ident(String)` |

标识符只认 ASCII：首字符 `is_ascii_alphabetic()` 或 `_`，后续字符 `is_ascii_alphanumeric()` 或 `_`。
字面量是 `Int(i64)` / `Float(f64)` / `Str(String)`，f-string 是 `FStr(Vec<FStrPart>)`；标点与运算符
各有变体（`LParen` `RParen` `LBracket` `RBracket` `Semi` `Dot` `Colon` `Arrow` `Assign` `Eq` `Ne`
`Lt` `Le` `Gt` `Ge` `Plus` `Minus` `Star` `Slash` `Percent` `Bang` `Comma`），布局是
`Newline` / `Indent` / `Dedent`，末尾是 `Eof`。注意 `Semi` 只服务于 `[T; N]` 类型语法。

### 4. 字面量

#### 4.1 整数与浮点

- 只扫描 ASCII 数字。`.` 只有在**后面紧跟数字**时才算小数点，所以 `1.5` 是浮点，而 `1.foo` 是
  `Int(1)` + `Dot` + `Ident("foo")`（随后由解析器当成字段访问）。
- 没有十六进制 / 二进制 / 下划线分隔符 / 指数记数法，也没有带符号字面量：`-1` 是解析器的一元负号。
- `i64` 放不下时报
  `integer literal '…' out of range (max 9223372036854775807)`；浮点解析失败报
  `invalid float literal '…'`。

#### 4.2 字符串与转义

字符串只用双引号，转义表只有四项：`\n`、`\t`、`\\`、`\"`（注意**没有** `\r`）。非法转义报
`unknown escape sequence '\x'`，没有收尾引号报 `unterminated string literal`。字符串是不可变的字节
序列，长度按字节算。

#### 4.3 f-string：`FStrPart` 切分与脱糖

`scan_word` 扫到一个单独的 `f`（或 `F`）且下一个字符是 `"` 时，切换到 f-string 扫描
（`f` 本身仍可以是普通标识符；`fx"` 也是标识符 `fx` 加字符串）。扫描规则：

- 普通字符进当前字面片段；`{{` / `}}` 产生字面量 `{` / `}`；单独的 `}` 报
  `single '}' in f-string (use '}}' for a literal brace)`。
- `{` 开始一个插值：用一个深度计数器跟踪 `(`/`[`，`}` 在深度为 0 时结束；插值里的字符串字面量被
  原样跳过（含 `\` 保护）。空插值（`{}` 或只有空白）报 `empty '{}' in f-string`，没等到 `}` 报
  `unterminated '{' in f-string`，整个 f-string 没结束报 `unterminated f-string literal`。
- 转义规则与普通字符串相同（`\n` `\t` `\\` `\"`）。
- 切分的结果是一串 `FStrPart`：纯字面量是 `Lit(String)`，插值部分是 `ExprTokens(Vec<Token>)`。

插值的子词法化是**原地**做的：`lex_interpolation` 直接在主字符缓冲区上取 `{` 与 `}` 之间的切片，
用一个 `paren_depth = 1` 的“虚拟括号”开局。这样做的两个后果：

- 缩进追踪对插值完全关闭，插值里出现换行也不会产生 `Newline`/`Indent`；
- 起始列设为 `{` 的列 + 1，所以插值内每个 token 的**列号和源码列号一致**（这也是它不需要复制、
  也不需要给片段补 padding 的原因）。子扫描的行号从 1 重新开始，因此在插值内报错时行号是相对片段
  的、列号是准确的。

解析器拿到 `FStr` 后立刻脱糖（`desugar_fstring`），生成一条左结合的 `+` 链：

```aoxn
def main() -> int:
    n = 3
    # f"n={n}!" 脱糖成 "n=" + str(n) + "!"
    print(f"n={n}!")
    return 0
```

即 `"lit" + str(expr) + "lit" + …`：每个插值变成一次 `str(...)` 调用（`Expr::Call { name: "str" }`），
空字面量片段被丢掉；如果整条 f-string 没有任何片段，结果是 `Expr::Str("")`。`str` 是普通内建，类型
检查与代码生成按普通调用处理。缺少 `str` 适用的类型（例如数组）会在类型检查阶段被拒
（`rejects_fstring_of_array`）。

### 5. 语法：递归下降

#### 5.1 顶层

`parse_program` 先跳过行首的 `Newline`，然后在 `Eof` 之前循环分派四种声明（顺序不限，`import` 可以
写在文件任意位置）：

| 看到 | 解析成 |
|---|---|
| `Import` | `ImportDecl { path, pos }`（`import "…"`，路径必须是字符串字面量） |
| `Def` | `FnDecl`（`is_extern = false`） |
| `Extern` | `FnDecl`（`is_extern = true`，无函数体） |
| `Struct` | `StructDecl { name, fields, pos }` |

其它 token 一律报 `expected 'import', 'def' or 'struct' at top level`（`rejects_top_level_statement`
测试）。**顶层不能写语句**：`print(1)` 在顶层是语法错误，这是 Python 与 Aoxn 的一个明显差异。

#### 5.2 块与语句

块只有两种写法（`block()`）：

```text
 ':' Newline Indent stmt* Dedent      # 缩进块
 ':' simple_stmt Newline              # 单行简单语句，例如 if n < 2: return n
```

单行形式里出现复合语句（`if` / `while` / `for` / `def` / `struct`）会报
`compound statements cannot appear on the same line after ':'`。缩进块里遇到 `Eof` 而不是 `Dedent`
报 `unexpected end of file inside an indented block`。

语句的形式与对应 AST：

| 源码 | AST |
|---|---|
| `x = e` / `x: t = e` | `Stmt::Let { name, ty, expr, pos }`（`ty` 只有注解形式才非空） |
| `a[i] = e` / `p.f = e` | `Stmt::Assign { target, expr, pos }` |
| `if` / `elif` / `else` | `Stmt::If { cond, then_block, else_block, pos }` |
| `while c:` | `Stmt::While { cond, body, pos }` |
| `for v in range(...)` / `for v in arr:` | `Stmt::For { var, iter, body, pos }` |
| `break` / `continue` | `Stmt::Break { pos }` / `Stmt::Continue { pos }` |
| `return` / `return e` | `Stmt::Return { expr, pos }` |
| `pass` | `Stmt::Pass` |
| 其它表达式 | `Stmt::ExprStmt { expr }` |

赋值语句的判别很直接：先解析一个表达式，如果后面跟着 `=`，那么目标是变量时归约为 `Stmt::Let`
（首次出现即声明），是下标/字段时归约为 `Stmt::Assign`；不是合法左值（`Expr::is_lvalue()` 只看变量、
下标、字段）就报 `invalid assignment target`。`x: t = e` 通过“`Ident` 后面紧跟 `:`”这一条前瞻识别。

#### 5.3 `if` / `elif` / `else`、`while`、`for`

- `elif` 在 AST 里**不存在**：解析器依次收下各分支，再自右向左折成嵌套 `Stmt::If`
  （`branches[0]` 是最外层），`else` 成为最内层的 `else_block`。这样 AST 节点很少，代价是 `elif`
  链在代码生成时表现为嵌套分支。
- `while` 直接解析成 `Stmt::While { cond, body, pos }`，循环体同样走 `block()`。
- `for` 的迭代源是**上下文相关**的：只有当 `for` 后面的标识符正好叫 `range` 且紧跟 `(` 时，才解析成
  `ForIter::Range(Vec<Expr>)`（1–3 个实参，参数名被丢弃）；否则解析一个表达式，当作数组迭代
  `ForIter::Array(Expr)`，元素按值复制进循环变量。所以一个名叫 `range` 的用户函数无法在 `for` 里被
  调用——它总被解释成区间语法。

#### 5.4 `def`、`extern def` 与泛型头

```text
 extern? 'def' Ident ('[' Ident (',' Ident)* ']')? '(' params? ')' ('->' ty)?
```

- 参数写成 `name: type`，逗号分隔，允许尾随逗号；返回类型注解可省略，缺省是 `Type::Void`。
- **参数类型注解是必需的**：`def f(x):` 会因为缺少 `:` 而报错。
- `extern def` 没有函数体：`body` 是空 `Block`，语句在换行处结束（`is_extern = true`）。
- 泛型头 `def sort[T, N](arr: [T; N]) -> [T; N]:` 里的方括号是参数名声明表（类型参数与长度参数都写在
  这里）。解析器把这些名字收进 `cur_type_params`，并在遇到 `[T; N]` 时把用到的长度名记进
  `cur_len_params`；函数结束时 `FnDecl.len_param` 取其中第一个。**最多一个长度参数**，第二个会报
  `only one length parameter is supported (found 'N' as well)`。

#### 5.5 类型语法与 `GENERIC_LEN` 哨兵

| 源码 | `Type` |
|---|---|
| `int` `float` `bool` `string` `void` | `Type::Int` / `Float` / `Bool` / `Str` / `Void` |
| 其它标识符 | `Type::Struct(name)`（是否真的存在由类型检查判断） |
| `[T; 3]` | `Type::Array { elem, len: 3 }` |
| `[T; N]`（`N` 是已声明的类型参数） | `Type::Array { elem, len: GENERIC_LEN }` |

`GENERIC_LEN = usize::MAX`，是“长度待单态化”的哨兵值；`Type` 的 `Display` 对它会打印 `[T; N]`，便于
错误信息可读。数组长度必须是正整数：`0` 或负数报 `array length must be a positive integer`；用了没有
在函数头声明的名字则报下面这条（`[T; N]` 里的长度名必须出现在函数头的类型参数表里）：

```text
 unknown array length 'N' (length parameters must be declared in the fn header, e.g. def f[T, N](arr: [T; N]))
```

#### 5.6 `struct` 与 `import`

- `struct` 的字段**只能**写成缩进块：`struct P:` 之后必须换行 + 缩进，每行一个 `name: type`。字段块里
  遇到 `Eof` 报 `unexpected end of file inside struct body`。
- `import` 只记下 `ImportDecl { path, pos }`，不做任何路径解析——解析与去重是加载器 `load_program`
  的职责（见 [编译器架构](Compiler-Architecture.md) §5）。路径字符串可以是相对路径（相对**导入它的
  那个文件**）或绝对路径。

#### 5.7 表达式：优先级爬升

每个优先级一个函数，逐层下降，全部左结合：

| 层 | 函数 | 运算符 |
|---|---|---|
| 1（最低） | `or_expr` | `or` |
| 2 | `and_expr` | `and` |
| 3 | `eq_expr` | `==` `!=` |
| 4 | `rel_expr` | `<` `<=` `>` `>=` |
| 5 | `add_expr` | `+` `-` |
| 6 | `mul_expr` | `*` `/` `%`（并在这一层特判数组复制） |
| 7 | `unary_expr` | `-` `not`（右结合，可叠加） |
| 8 | `postfix` | 调用 `f(...)`、下标 `a[i]`、字段 `p.f`（后缀链） |
| 9（最高） | `primary` | 整数/浮点/字符串/`True`/`False`、标识符、f-string、`(e)`、`[e0, e1, …]` |

几个值得记住的细节：

- **数组复制**：`mul_expr` 在解析完 `*` 两侧后检查是不是 `[e] * N` 或 `N * [e]`（元素恰好一个、`N`
  是正整数），是则产生 `Expr::ArrayRep { elem, count, lit_id, pos }`，否则是普通乘法。
- **调用只能是具名函数**：后缀 `(` 只对 `Expr::Var` 生效，别的表达式报
  `only named functions can be called`。实参可以写 `name=value`（结构体构造需要，解析器只记录
  `Arg { name: Some(...) }`），也允许尾随逗号。
- **空数组字面量被拒**：`[]` 报
  `empty array literals are not allowed (element type could not be inferred)`。
- 括号只用于分组：`(e)` 直接返回内部的 `e`，不产生额外节点，所以内部位置信息原样保留。

### 6. `src/ast.rs` 的节点形状

AST 是纯数据结构，全部按值组织（`Box` 断开递归，`Vec` 装列表），没有 arena、没有指针、没有 span：

```text
 Program { imports: Vec<ImportDecl>, structs: Vec<StructDecl>, funcs: Vec<FnDecl> }

 FnDecl { name: String, type_params: Vec<String>, len_param: Option<String>,
          params: Vec<Param>, ret: Type, body: Block, is_extern: bool, pos: Pos }

 StructDecl { name: String, fields: Vec<Param>, pos: Pos }
 Param { name: String, ty: Type, pos: Pos }
 Block { stmts: Vec<Stmt> }

 Stmt ::= Let { name, ty: Option<Type>, expr, pos } | Assign { target, expr, pos }
        | If { cond, then_block, else_block: Option<Block>, pos }
        | While { cond, body, pos }
        | For { var, iter: ForIter, body, pos }
        | Break { pos } | Continue { pos } | Return { expr: Option<Expr>, pos }
        | Pass | ExprStmt { expr }

 ForIter ::= Range(Vec<Expr>) | Array(Expr)

 Expr ::= Int(i64, Pos) | Float(f64, Pos) | Str(String, Pos) | Bool(bool, Pos)
        | Var { name, pos }
        | Call { name, args: Vec<Arg>, pos, lit_id }
        | Unary { op: UnOp, expr: Box<Expr>, pos }
        | Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr>, pos }
        | Index { arr: Box<Expr>, idx: Box<Expr>, pos }
        | Field { obj: Box<Expr>, name, pos }
        | ArrayLit { elems: Vec<Expr>, lit_id, pos }
        | ArrayRep { elem: Box<Expr>, count: usize, lit_id, pos }
        | StructLit { name, fields: Vec<(String, Expr)>, lit_id, pos }

 Arg { name: Option<String>, value: Expr }
 BinOp = Add | Sub | Mul | Div | Mod | Eq | Ne | Lt | Le | Gt | Ge | And | Or
 UnOp  = Neg | Not
```

每个节点只带一个 `Pos`（取自该结构的关键 token：名字、运算符或字面量），没有结束位置，也没有
`NodeId`。`Expr::pos()` 统一取位置；`Expr::is_lvalue()` 判断变量/下标/字段。`Type` 有
`Int`/`Float`/`Bool`/`Str`/`Void`/`Array{elem,len}`/`Struct(name)`，外加 `is_printable()` 与
`is_compound()` 两个小谓词。

### 7. 前端给后端的隐含契约：AST 节点地址

AST 里那些 `lit_id` 是解析器分配的**每文件**自增 id（每个文件从 0 重新开始），但代码生成并不用它做
缓存键：`lit_temps` / `val_temps` / `iter_temps` 全部以 **AST 节点地址**为键
（`expr as *const Expr as usize`），泛型调用的 `call_map` 也是（类型检查写、代码生成读）。这带来两条
前端必须守住的约束：

1. **不要在两个阶段之间克隆表达式。** 克隆出来的 `Expr` 地址不同，缓存键就会失配；更糟的是克隆体
   被释放后地址可能被复用，导致某个临时量在另一个函数里被“认领”。需要传节点时借用（`&expr`）。
   `tests/pipeline.rs` 的 `import_aggregate_literals_across_files` 是一个回归测试：多文件编译时每个
   文件的 `lit_id` 都从 0 开始，聚合字面量的临时 alloca 因此必须按 AST 站点而不是 id 区分。
2. **泛型实例在“检查”和“发射”之间必须是同一批对象。** 类型检查在 pass 4 里检查的就是随后交给
   代码生成的 `FnDecl`（`instances`），如果中途被克隆，实例体内部的泛型调用就会丢掉 `call_map`
   条目。细节见 [编译器架构](Compiler-Architecture.md) §2.3 与 [类型检查](Type-Checker.md)。

### 8. 错误定位质量

- lexer 的 `Diag` 带上出错的 `line`/`col`/`file`；parser 的 `Diag` 用当前 token 的 `file` 加上显式的
  `line`/`col`，所以每条诊断都能落到「哪个文件的哪一行哪一列」。
- 多文件编译时每个文件有自己的 `file` 索引，所以来自被导入文件的错误仍然指向该文件自己的行号
  （`import_error_reports_importing_file` 断言诊断指向 `lib.ax:2` 且消息是
  `unknown variable 'unknown_var'`）；如果没有更具体的位置，加载器会把 import 语句本身的位置补上。
- 三个实测输出（路径做了简化）：

```text
 [lex] bad.ax:3:1: unindent does not match any outer indentation level (expected one of [0], found 3)
 [lex] unclosed.ax:4:1: unclosed bracket: '(' or '[' was never closed
 [parse] slash.ax:2:13: expected an expression, found Slash
```

默认形态是 `[stage] file:line:col: message`；`--json` 会给出同一条诊断的结构化版本，见
[命令行与工具链](CLI-and-Tooling.md)。

### 9. 例子：`examples/hello.ax` 的 token 流与 AST 示意

源码（[examples/hello.ax](../examples/hello.ax)）：

```aoxn
# the classic
def main() -> int:
    print("hello, Aoxn")
    return 0
```

token 流（19 个 token，**kind 与文本为实测输出**；由 `selfhost/lexer.ax` 打印，kind 顺序与文本同
Rust 词法器的规则一致）：

```text
 Def
 Ident("main")
 LParen
 RParen
 Arrow
 TyInt
 Colon
 Newline
 Indent
 Ident("print")
 LParen
 Str("hello, Aoxn")
 RParen
 Newline
 Return
 Int(0)
 Newline
 Dedent
 Eof
```

两处值得注意：第一行的注释**没有**产生任何 token（连 `Newline` 都没有），所以流从 `Def` 开始；
`Indent` 出现在 `Newline` 之后——这正是解析器 `block()` 期待的 `:` Newline Indent stmt* Dedent 形状。

同一程序的 AST（**示意**：按 §6 的结构手写展开，不是某次真实 dump 的逐字输出）：

```text
 Program { imports: [], structs: [], funcs: [
   FnDecl {
     name: "main", type_params: [], len_param: None, params: [],
     ret: Type::Int, is_extern: false,
     body: Block { stmts: [
       Stmt::ExprStmt { expr: Expr::Call { name: "print",
                            args: [Arg { name: None, value: Expr::Str("hello, Aoxn") }],
                            lit_id: 0 } },
       Stmt::Return { expr: Some(Expr::Int(0)) },
     ] },
   },
 ] }
```

（自举侧的真实 dump 格式可以在 `selfhost/parse_demo.ax` 与 `selfhost_parser_ast_dump` 测试里看到：
它打印 `BLOCK program` / `FN main` 这样的标签树。）

### 10. 自举侧的前端

Aoxn 自己写的前端在 [selfhost/lexer.ax](../selfhost/lexer.ax)（阶段 1）与
[selfhost/parser.ax](../selfhost/parser.ax)（阶段 2），前者是 Rust 词法器的忠实移植，后者的 AST 用
**arena** 表示：每个节点是并行 `Vec` 里的一个下标（tag / sval / ival / child / next），子节点用
first-child + next-sibling 串起来，而不是 `Box`。两者的行为都由逐字节比较期望输出的测试钉住
（`selfhost_lexer_token_stream`、`selfhost_parser_ast_dump`）。

一个需要注意的差异：自举 lexer 的 `emit` 记录的是**发射时刻**的列号，所以多字符 token 的列号是扫描
结束之后的列（实测 `def` 报 `2:4`），而 Rust 侧 `Pos` 是 token 起始列（`def` 是 `2:1`）；两个实现之间
做比较的测试只比 kind 与文本，不含位置。自举前端的完整状态（含固定点）见
[自举](Self-Hosting.md)。

## English

### 1. Where this sits in the pipeline

```text
 .ax source text
      │  src/lexer.rs ::  lex(src: &str, file_id: u32) -> Result<Vec<Token>, Diag>
      ▼  Token { tok: Tok, pos: Pos{ line, col, file } }; incl. Newline / Indent / Dedent
 token stream
      │  src/parser.rs :: parse(tokens: Vec<Token>) -> Result<Program, Diag>
      ▼  Program from src/ast.rs (recursive descent, one-token lookahead)
 AST
      │  → src/typecheck.rs → src/codegen.rs
```

The front end does exactly two things: turn characters into tokens, then turn tokens into a tree. No
semantic judgement (types, return paths, undefined variables) happens here. Both stages return
`Result<_, Diag>` and stop at the first error — there is **no error recovery: one compilation reports
one diagnostic**.

### 2. Layout-aware lexing

#### 2.1 The indent stack: how Indent / Dedent are produced

The lexer owns an `indent_stack` that starts as `vec![0]`. At the start of a line (and only outside
brackets) it measures the leading indentation:

```text
 ' '  → indent = indent + 1
 '\t' → indent = (indent / 4 + 1) * 4      # tabs snap to the next 4-column boundary
 other → stop measuring
```

It then compares that with the stack top:

- `indent > top`: push and emit one `Indent` (position pinned to "this line, col 1").
- `indent < top`: pop repeatedly, emitting one `Dedent` per pop; if the stack top still differs from
  `indent`, report
  `unindent does not match any outer indentation level (expected one of […], found n)`.
- Equal: emit nothing (a normal statement line).

Only the indentation width is compared; nothing requires a specific step size, just consistency within
one block.

#### 2.2 Blank lines and comment-only lines produce no tokens at all

After measuring indentation the lexer looks at the first character of the line:

- `\n` (or `\r\n`): a blank line — skip it. **No `Newline` token, no indent change.**
- `#`: skip the comment to end of line — again **no token whatsoever**.
- Anything else: handle indentation per 2.1, then scan the line normally.

This is easy to overlook: a leading comment line produces no `Newline`, so the token stream of
`examples/hello.ax` starts with `Def` (see the measured output in §9).

#### 2.3 Inside brackets, newlines and indentation are ignored (implicit joining)

`(` and `[` increment `paren_depth`; `)` and `]` decrement it (they are interchangeable — a `]` will
close a `(`). While `paren_depth > 0`:

- `\n` produces no `Newline` and does not set `at_line_start`, so the whole indentation-measuring
  block is skipped and leading whitespace is ordinary whitespace.
- Consequently the arguments of `print(` may span any number of lines, one per line, with a trailing
  comma allowed (`multiline_call_arguments` in `tests/pipeline.rs` exercises this).

Two edge cases are reported explicitly:

- More closers than openers (`paren_depth` goes negative): `unmatched closing ')'`.
- `paren_depth` still positive at end of file: `unclosed bracket: '(' or '[' was never closed`, located
  at EOF (observed at the line after the last one, e.g. `4:1`).

The one exception is the f-string interpolation sub-scan: it opens with a "virtual paren"
(`virtual_paren`, `paren_depth = 1`) so it never reports an unclosed bracket of its own (see §4.3).

#### 2.4 Finishing at EOF

After the scan loop ends, the tail is completed in a fixed order:

```text
 1. if the last token is not Newline → append one Newline (at the current line/col)
 2. while the indent stack is deeper than 1 → pop and append one Dedent each (same EOF position)
 3. append one Eof
```

So every valid token stream ends with `… Eof`, and the token before `Eof` is always a `Dedent` or a
`Newline`.

#### 2.5 The only comment marker is `#`

`#` is handled both in the line-start branch and in the main loop; both skip to end of line. There are
no block comments and no doc comments.

`//` is **not** a comment: the lexer has no such rule, `'/'` is a single-character token, so `17 // 5`
yields two `Tok::Slash` tokens. The parser maps **one** `Slash` to `BinOp::Div`, and `/` on `int` is
already truncating division (`17 / 5` is `3`, see `arithmetic_and_precedence` in `tests/pipeline.rs`).
The repo docs (`CONTRIBUTING.md`) describe `//` as "integer division", but the v0.26.3 implementation
has no `//` token: `17 // 5` fails with `expected an expression, found Slash`. **Write `/` for
division.**

### 3. Token structure and positions

```text
 pub struct Token {
     pub tok: Tok,
     pub pos: Pos,          // { line, col, file }
 }

 pub struct Pos {
     pub line: usize,       // 1-based
     pub col: usize,        // 1-based; a tab advances 4 columns
     pub file: u32,         // index into this compilation's file registry (src/files.rs)
 }

 pub enum FStrPart {
     Lit(String),                  // a literal chunk of an f-string
     ExprTokens(Vec<Token>),       // an interpolation, already sub-lexed
 }
```

`Pos` is captured **before** a token is scanned, so it is the token's start position; `Indent` is the
one exception, with its column pinned to 1. `file` is only an index — human-readable names live in the
`src/files.rs` registry and are resolved when `diag_to_string` prints or `diags_to_json` serializes
(see [Compiler Architecture](Compiler-Architecture.md) §4).

`Tok` is one flat enum (no nested payload structs); keywords are folded into dedicated variants right
here:

| Source spelling | Token |
|---|---|
| `def` `struct` `extern` `import` `if` `elif` `else` `while` `for` `in` | `Def` `Struct` `Extern` `Import` `If` `Elif` `Else` `While` `For` `In` |
| `break` `continue` `return` `pass` | `Break` `Continue` `Return` `Pass` |
| `and` `or` `not` | `AndAnd` `OrOr` `Bang` (**no** dedicated keyword tokens) |
| `true` `True` / `false` `False` | `True` / `False` |
| `int` `float` `bool` `string` `void` | `TyInt` `TyFloat` `TyBool` `TyString` `TyVoid` |
| any other identifier | `Ident(String)` |

Identifiers are ASCII-only: the first character must satisfy `is_ascii_alphabetic()` or be `_`, later
characters `is_ascii_alphanumeric()` or `_`. Literals are `Int(i64)` / `Float(f64)` / `Str(String)`,
and an f-string is `FStr(Vec<FStrPart>)`; punctuation and operators each have a variant
(`LParen` `RParen` `LBracket` `RBracket` `Semi` `Dot` `Colon` `Arrow` `Assign` `Eq` `Ne` `Lt` `Le`
`Gt` `Ge` `Plus` `Minus` `Star` `Slash` `Percent` `Bang` `Comma`), layout is
`Newline` / `Indent` / `Dedent`, and the stream ends with `Eof`. Note that `Semi` exists only to serve
the `[T; N]` type syntax.

### 4. Literals

#### 4.1 Integers and floats

- Only ASCII digits are scanned. A `.` counts as a decimal point only when the **next character is a
  digit**, so `1.5` is a float while `1.foo` is `Int(1)` + `Dot` + `Ident("foo")` (which the parser
  then reads as a field access).
- There is no hexadecimal / binary / underscore separator / exponent notation, and no signed literal:
  `-1` is a unary minus in the parser.
- An `i64` overflow reports
  `integer literal '…' out of range (max 9223372036854775807)`; a failed float parse reports
  `invalid float literal '…'`.

#### 4.2 Strings and escapes

Strings use double quotes only, and the escape table has exactly four entries: `\n`, `\t`, `\\`, `\"`
(note: **no** `\r`). An illegal escape reports `unknown escape sequence '\x'`; a missing closing quote
reports `unterminated string literal`. Strings are immutable byte sequences and lengths are counted in
bytes.

#### 4.3 f-strings: splitting into `FStrPart` and desugaring

When `scan_word` reads a lone `f` (or `F`) followed by `"`, it switches to f-string scanning (a plain
`f` is still an ordinary identifier, and `fx"` is the identifier `fx` followed by a string). The scan
rules are:

- Ordinary characters go into the current literal chunk; `{{` / `}}` produce a literal `{` / `}`; a
  lone `}` reports `single '}' in f-string (use '}}' for a literal brace)`.
- `{` starts an interpolation: a depth counter tracks `(`/`[`, and `}` at depth 0 ends it. String
  literals inside the interpolation are skipped verbatim (with `\` guarding). An empty interpolation
  (`{}` or whitespace only) reports `empty '{}' in f-string`; a missing `}` reports
  `unterminated '{' in f-string`; an unterminated f-string reports `unterminated f-string literal`.
- Escapes work exactly as in plain strings (`\n` `\t` `\\` `\"`).
- The result is a list of `FStrPart`s: pure text becomes `Lit(String)`, an interpolation becomes
  `ExprTokens(Vec<Token>)`.

Interpolations are sub-lexed **in place**: `lex_interpolation` takes the slice between `{` and `}` from
the main character buffer and starts with a "virtual paren" (`paren_depth = 1`). Two consequences:

- Indent tracking is fully disabled for the interpolation, so a newline inside it produces no
  `Newline`/`Indent`.
- The starting column is the column of `{` plus one, so every token inside the interpolation keeps the
  **source column** (which is also why no copy and no padding of the snippet is needed). Line numbers
  restart at 1 inside the sub-scan, so an error inside an interpolation has an accurate column and a
  snippet-relative line.

The parser desugars an `FStr` immediately (`desugar_fstring`) into a left-associative `+` chain:

```aoxn
def main() -> int:
    n = 3
    # f"n={n}!" desugars to "n=" + str(n) + "!"
    print(f"n={n}!")
    return 0
```

That is `"lit" + str(expr) + "lit" + …`: each interpolation becomes one `str(...)` call
(`Expr::Call { name: "str" }`) and empty literal chunks are dropped; an f-string with no parts at all
becomes `Expr::Str("")`. `str` is an ordinary builtin, so type checking and code generation treat the
result as a normal call. Types `str` cannot handle (arrays, for example) are rejected by the type
checker (`rejects_fstring_of_array`).

### 5. Grammar: recursive descent

#### 5.1 Top level

`parse_program` first skips leading `Newline`s, then loops over four kinds of declaration until `Eof`
(in any order — an `import` may appear anywhere in the file):

| Next token | Parsed as |
|---|---|
| `Import` | `ImportDecl { path, pos }` (`import "…"`; the path must be a string literal) |
| `Def` | `FnDecl` (`is_extern = false`) |
| `Extern` | `FnDecl` (`is_extern = true`, no body) |
| `Struct` | `StructDecl { name, fields, pos }` |

Anything else reports `expected 'import', 'def' or 'struct' at top level` (see the
`rejects_top_level_statement` test). **Statements are not allowed at top level**: a bare `print(1)` is
a syntax error, a noticeable difference from Python.

#### 5.2 Blocks and statements

There are exactly two block forms (`block()`):

```text
 ':' Newline Indent stmt* Dedent      # an indented block
 ':' simple_stmt Newline              # one simple statement, e.g. if n < 2: return n
```

A compound statement (`if` / `while` / `for` / `def` / `struct`) in the single-line form reports
`compound statements cannot appear on the same line after ':'`. Hitting `Eof` instead of `Dedent`
inside an indented block reports `unexpected end of file inside an indented block`.

Statement forms and their AST nodes:

| Source | AST |
|---|---|
| `x = e` / `x: t = e` | `Stmt::Let { name, ty, expr, pos }` (`ty` is non-empty only in the annotated form) |
| `a[i] = e` / `p.f = e` | `Stmt::Assign { target, expr, pos }` |
| `if` / `elif` / `else` | `Stmt::If { cond, then_block, else_block, pos }` |
| `while c:` | `Stmt::While { cond, body, pos }` |
| `for v in range(...)` / `for v in arr:` | `Stmt::For { var, iter, body, pos }` |
| `break` / `continue` | `Stmt::Break { pos }` / `Stmt::Continue { pos }` |
| `return` / `return e` | `Stmt::Return { expr, pos }` |
| `pass` | `Stmt::Pass` |
| any other expression | `Stmt::ExprStmt { expr }` |

Assignments are disambiguated directly: parse an expression, and if an `=` follows, a variable target
folds into `Stmt::Let` (first occurrence declares) while an index/field target folds into
`Stmt::Assign`; a target that is not a valid lvalue (`Expr::is_lvalue()` accepts variables, indexes
and fields only) reports `invalid assignment target`. The annotated form `x: t = e` is recognised by
the one-token lookahead "`Ident` immediately followed by `:`".

#### 5.3 `if` / `elif` / `else`, `while`, `for`

- `elif` **does not exist** in the AST: the parser collects every branch and folds them right-to-left
  into nested `Stmt::If`s (`branches[0]` is the outermost), with `else` becoming the `else_block` of
  the innermost one. That keeps the node count low at the price of `elif` chains appearing as nested
  branches during code generation.
- `while` parses straight into `Stmt::While { cond, body, pos }`, with the body going through
  `block()` as well.
- The iterator of a `for` is **contextual**: only when the identifier right after `for` is exactly
  `range` and is immediately followed by `(` does it parse as `ForIter::Range(Vec<Expr>)` (1–3
  arguments, whose names are dropped); otherwise it parses one expression and iterates an array
  (`ForIter::Array(Expr)`), copying each element into the loop variable. A user function named `range`
  therefore cannot be called from a `for` — it is always read as the range syntax.

#### 5.4 `def`, `extern def`, and generic headers

```text
 extern? 'def' Ident ('[' Ident (',' Ident)* ']')? '(' params? ')' ('->' ty)?
```

- Parameters are `name: type`, comma separated, with a trailing comma allowed; the return annotation
  may be omitted and defaults to `Type::Void`.
- **Parameter type annotations are mandatory**: `def f(x):` fails on the missing `:`.
- `extern def` has no body: `body` is an empty `Block` and the statement ends at the newline
  (`is_extern = true`).
- The generic header in `def sort[T, N](arr: [T; N]) -> [T; N]:` is the bracketed declaration list of
  parameter names (both type and length parameters are written there). The parser records them in
  `cur_type_params` and, when it meets `[T; N]`, records the length name in `cur_len_params`; at the end
  of the function `FnDecl.len_param` takes the first of them. **At most one length parameter** is
  allowed: a second one reports `only one length parameter is supported (found 'N' as well)`.

#### 5.5 Type syntax and the `GENERIC_LEN` sentinel

| Source | `Type` |
|---|---|
| `int` `float` `bool` `string` `void` | `Type::Int` / `Float` / `Bool` / `Str` / `Void` |
| any other identifier | `Type::Struct(name)` (whether it exists is the type checker's call) |
| `[T; 3]` | `Type::Array { elem, len: 3 }` |
| `[T; N]` (`N` declared as a type parameter) | `Type::Array { elem, len: GENERIC_LEN }` |

`GENERIC_LEN = usize::MAX` is the "length to be monomorphized" sentinel; `Type`'s `Display` prints it
as `[T; N]` so diagnostics stay readable. An array length must be a positive integer: `0` or a negative
value reports `array length must be a positive integer`, while a name that was never declared in the
function header reports:

```text
 unknown array length 'N' (length parameters must be declared in the fn header, e.g. def f[T, N](arr: [T; N]))
```

#### 5.6 `struct` and `import`

- `struct` fields **must** be an indented block: after `struct P:` there must be a newline plus
  indentation, one `name: type` per line. Hitting `Eof` inside the field block reports
  `unexpected end of file inside struct body`.
- `import` only records `ImportDecl { path, pos }` and performs no path resolution whatsoever —
  resolving and deduplicating is the loader's job in `load_program` (see
  [Compiler Architecture](Compiler-Architecture.md) §5). The path string may be relative (to the file
  that contains the import) or absolute.

#### 5.7 Expressions: precedence climbing

One function per precedence level, each delegating downward, all left-associative:

| Level | Function | Operators |
|---|---|---|
| 1 (lowest) | `or_expr` | `or` |
| 2 | `and_expr` | `and` |
| 3 | `eq_expr` | `==` `!=` |
| 4 | `rel_expr` | `<` `<=` `>` `>=` |
| 5 | `add_expr` | `+` `-` |
| 6 | `mul_expr` | `*` `/` `%` (plus the array-replication special case) |
| 7 | `unary_expr` | `-` `not` (right-associative, stackable) |
| 8 | `postfix` | call `f(...)`, index `a[i]`, field `p.f` (a suffix chain) |
| 9 (highest) | `primary` | int/float/string/`True`/`False`, identifier, f-string, `(e)`, `[e0, e1, …]` |

Details worth remembering:

- **Array replication**: after parsing both sides of `*`, `mul_expr` checks for `[e] * N` or `N * [e]`
  (exactly one element and a positive integer `N`); if so it produces
  `Expr::ArrayRep { elem, count, lit_id, pos }`, otherwise a plain multiplication.
- **Only named functions are callable**: the `(` suffix applies to `Expr::Var` alone, and anything else
  reports `only named functions can be called`. Arguments may be written `name=value` (struct
  construction needs it; the parser just records `Arg { name: Some(...) }`), and a trailing comma is
  allowed.
- **Empty array literals are rejected**: `[]` reports
  `empty array literals are not allowed (element type could not be inferred)`.
- Parentheses only group: `(e)` returns the inner `e` directly, so no extra node is created and inner
  positions are preserved.

### 6. Node shapes in `src/ast.rs`

The AST is plain data, organised by value (`Box` breaks recursion, `Vec` holds lists): no arena, no
pointers, no spans.

```text
 Program { imports: Vec<ImportDecl>, structs: Vec<StructDecl>, funcs: Vec<FnDecl> }

 FnDecl { name: String, type_params: Vec<String>, len_param: Option<String>,
          params: Vec<Param>, ret: Type, body: Block, is_extern: bool, pos: Pos }

 StructDecl { name: String, fields: Vec<Param>, pos: Pos }
 Param { name: String, ty: Type, pos: Pos }
 Block { stmts: Vec<Stmt> }

 Stmt ::= Let { name, ty: Option<Type>, expr, pos } | Assign { target, expr, pos }
        | If { cond, then_block, else_block: Option<Block>, pos }
        | While { cond, body, pos }
        | For { var, iter: ForIter, body, pos }
        | Break { pos } | Continue { pos } | Return { expr: Option<Expr>, pos }
        | Pass | ExprStmt { expr }

 ForIter ::= Range(Vec<Expr>) | Array(Expr)

 Expr ::= Int(i64, Pos) | Float(f64, Pos) | Str(String, Pos) | Bool(bool, Pos)
        | Var { name, pos }
        | Call { name, args: Vec<Arg>, pos, lit_id }
        | Unary { op: UnOp, expr: Box<Expr>, pos }
        | Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr>, pos }
        | Index { arr: Box<Expr>, idx: Box<Expr>, pos }
        | Field { obj: Box<Expr>, name, pos }
        | ArrayLit { elems: Vec<Expr>, lit_id, pos }
        | ArrayRep { elem: Box<Expr>, count: usize, lit_id, pos }
        | StructLit { name, fields: Vec<(String, Expr)>, lit_id, pos }

 Arg { name: Option<String>, value: Expr }
 BinOp = Add | Sub | Mul | Div | Mod | Eq | Ne | Lt | Le | Gt | Ge | And | Or
 UnOp  = Neg | Not
```

Each node carries exactly one `Pos`, taken from the key token of that construct (a name, an operator or
a literal) — no end position, no node ids. `Expr::pos()` fetches the position uniformly, and
`Expr::is_lvalue()` recognises variables, indexes and fields. `Type` has
`Int`/`Float`/`Bool`/`Str`/`Void`/`Array{elem,len}`/`Struct(name)`, plus the small predicates
`is_printable()` and `is_compound()`.

### 7. The front end's implicit contract with the back end: AST node addresses

The `lit_id` fields in the AST are a **per-file** counter allocated by the parser (each file restarts
at 0), but code generation does not key its caches on them: `lit_temps` / `val_temps` / `iter_temps`
are all keyed by **AST node address** (`expr as *const Expr as usize`), and so is the `call_map` for
generic calls (written by the type checker, read by codegen). Two constraints follow for the front end:

1. **Do not clone expressions between the phases.** A clone has a different address, so cache keys miss
   it; worse, once a clone is freed its address can be recycled and a temporary gets claimed by another
   function. Pass nodes by reference (`&expr`) instead. `import_aggregate_literals_across_files` in
   `tests/pipeline.rs` is the regression test for this: because per-file `lit_id`s restart at 0,
   aggregate literal temporaries must be distinguished by AST site rather than by id.
2. **Generic instances must be the same objects during checking and emission.** Type checking pass 4
   checks exactly the `FnDecl`s it later hands to codegen (`instances`); if they were cloned in
   between, generic calls inside an instance body would lose their `call_map` entries. See
   [Compiler Architecture](Compiler-Architecture.md) §2.3 and [Type Checker](Type-Checker.md).

### 8. Diagnostic quality

- A lexer `Diag` carries the offending `line`/`col`/`file`; a parser `Diag` uses the current token's
  `file` plus explicit `line`/`col`. Every diagnostic therefore lands on a file, a line and a column.
- In a multi-file compilation each file has its own `file` index, so an error coming from an imported
  file still points at that file's own line numbers (`import_error_reports_importing_file` asserts the
  diagnostic points at `lib.ax:2` with the message `unknown variable 'unknown_var'`); when no more
  specific position exists, the loader fills in the position of the import statement itself.
- Three observed outputs (paths shortened):

```text
 [lex] bad.ax:3:1: unindent does not match any outer indentation level (expected one of [0], found 3)
 [lex] unclosed.ax:4:1: unclosed bracket: '(' or '[' was never closed
 [parse] slash.ax:2:13: expected an expression, found Slash
```

The default form is `[stage] file:line:col: message`; `--json` produces the structured version of the
same diagnostic, see [CLI and Tooling](CLI-and-Tooling.md).

### 9. Example: the token stream and AST of `examples/hello.ax`

The source ([examples/hello.ax](../examples/hello.ax)):

```aoxn
# the classic
def main() -> int:
    print("hello, Aoxn")
    return 0
```

The token stream (19 tokens; **kinds and text are measured output**, printed by
`selfhost/lexer.ax`, whose kind order and text match the Rust lexer's rules):

```text
 Def
 Ident("main")
 LParen
 RParen
 Arrow
 TyInt
 Colon
 Newline
 Indent
 Ident("print")
 LParen
 Str("hello, Aoxn")
 RParen
 Newline
 Return
 Int(0)
 Newline
 Dedent
 Eof
```

Two things stand out: the comment on the first line produces **no** token at all (not even a
`Newline`), so the stream starts with `Def`; and `Indent` follows `Newline` — exactly the
`:` Newline Indent stmt* Dedent shape `block()` expects.

The AST of the same program (**schematic**: expanded by hand following §6, not a verbatim dump):

```text
 Program { imports: [], structs: [], funcs: [
   FnDecl {
     name: "main", type_params: [], len_param: None, params: [],
     ret: Type::Int, is_extern: false,
     body: Block { stmts: [
       Stmt::ExprStmt { expr: Expr::Call { name: "print",
                            args: [Arg { name: None, value: Expr::Str("hello, Aoxn") }],
                            lit_id: 0 } },
       Stmt::Return { expr: Some(Expr::Int(0)) },
     ] },
   },
 ] }
```

(The real dump format of the self-hosted side can be seen in `selfhost/parse_demo.ax` and the
`selfhost_parser_ast_dump` test: it prints a labelled tree such as `BLOCK program` / `FN main`.)

### 10. The self-hosted front end

The Aoxn-written front end lives in [selfhost/lexer.ax](../selfhost/lexer.ax) (stage 1) and
[selfhost/parser.ax](../selfhost/parser.ax) (stage 2). The former is a faithful port of the Rust lexer;
the latter represents the AST as an **arena**: every node is an index into parallel `Vec`s
(tag / sval / ival / child / next) and children are chained first-child + next-sibling instead of
`Box`. Both are pinned down by tests that compare the expected output byte for byte
(`selfhost_lexer_token_stream`, `selfhost_parser_ast_dump`).

One difference to be aware of: the self-hosted lexer's `emit` records the column at **emit time**, so a
multi-character token gets the column after the scan (measured: `def` reports `2:4`), while the Rust
`Pos` is the token's start column (`def` is `2:1`). The tests that compare the two implementations
compare kinds and text only, not positions. The full state of self-hosting (including the fixed point)
is in [Self-Hosting](Self-Hosting.md).

---

## 源文件 / Source files

- [src/lexer.rs](../src/lexer.rs) — indent stack, Newline/Indent/Dedent, escapes, f-string splitting
- [src/parser.rs](../src/parser.rs) — recursive descent, blocks, statements, precedence, desugaring
- [src/ast.rs](../src/ast.rs) — `Program` / `Stmt` / `Expr` / `Type` / `Pos`, `GENERIC_LEN`
- [src/files.rs](../src/files.rs) — the file registry behind `Pos::file`
- [src/lib.rs](../src/lib.rs) — where `lex` and `parse` are called, and how their `Diag`s flow out
- [src/typecheck.rs](../src/typecheck.rs) — consumers of `type_params`, `len_param`, `GENERIC_LEN`
- [src/codegen.rs](../src/codegen.rs) — the temp caches and `call_map` keyed by AST node address
- [tests/pipeline.rs](../tests/pipeline.rs) — parser/lexer behaviour tests and the import suite
- [examples/hello.ax](../examples/hello.ax) — the sample program used in §9
- [selfhost/lexer.ax](../selfhost/lexer.ax) — the Aoxn-written lexer and its token dump
- [selfhost/parser.ax](../selfhost/parser.ax) — the Aoxn-written parser with the arena AST
- [selfhost/lex_demo.ax](../selfhost/lex_demo.ax) — token-stream demo behind the stage-1 test
- [selfhost/parse_demo.ax](../selfhost/parse_demo.ax) — AST-dump demo behind the stage-2 test
- [docs/spec.md](../docs/spec.md) — the language spec (its `Statements` section is stale)
- [CONTRIBUTING.md](../CONTRIBUTING.md) — Aoxn code conventions (`#` comments, 4-space indent)
