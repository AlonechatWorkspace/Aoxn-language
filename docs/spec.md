# Aoxn Language Specification (v0.29.7)

Aoxn is an AI-native, statically typed, ahead-of-time compiled language with a
Python-style syntax. Design goals: minimal syntax, explicit semantics, native
(C++-class) speed via clang-compiled C, self-hosting core libraries, and
machine-friendly tooling (JSON diagnostics, dumpable C text).

The surface syntax tracks Python closely: the same operators, the same
control-flow shapes, the same indentation rules. Where Aoxn deliberately
differs (strict static typing, value semantics, no implicit numeric mixing)
the deviation is called out explicitly below.

## Layout rules (Python-style)

- Blocks are defined by **indentation**, not braces. Any consistent width works;
  a tab counts as 4 columns.
- Statements end at a line break; no semicolons. `;` is a syntax error.
- `#` starts a comment (to end of line). Blank and comment-only lines are ignored.
- Inside `(...)` a line break is ignored (implicit line joining) 鈥?call
  arguments may span lines, trailing commas allowed.
- A single simple statement may follow `:` on the same line
  (`if n < 2: return n`). Compound statements (if/while/def) cannot.

## Types

| Type     | Meaning                  | Backend       |
|----------|--------------------------|---------------|
| `int`    | signed 64-bit integer    | i64           |
| `float`  | 64-bit IEEE float        | double        |
| `bool`   | `True` / `False`         | i1            |
| `string` | immutable byte string    | ptr to NUL-terminated bytes (heap when built, static when literal) |
| `void`   | absence of a value       | (function return only) |
| `[T; N]` | fixed-size array, N > 0  | [N x T]       |
| `Name`   | struct (see below)       | named %struct |

No implicit conversions. `int` and `float` never mix silently; `%` is int-only.
Integer division by zero is undefined (native crash, no runtime check - speed
first, matching C/C++). Signed integer overflow is undefined as well, since
`int` arithmetic lowers to plain C `long long` arithmetic.
Array indexing is **unchecked** (C-style).

## Arrays

```Aoxn
a = [10, 20, 30]              # inferred [int; 3]
xs: [int; 4] = [1, 2, 3, 4]   # annotated
m: [[int; 2]; 3] = [[1, 2], [3, 4], [5, 6]]   # nested
a[1] = 99                     # element assignment
print(len(a))                 # 3 (compile-time constant)
```

- Literals are non-empty; all elements must share one type (compounds allowed:
  arrays of structs, arrays of arrays).
- Indexing is 0-based with an `int`; **no bounds checking** (out-of-bounds is
  undefined behavior, like C).
- `len(x)` requires an array and returns its (static) length as `int`.

## Structs

```Aoxn
struct Point:
    x: int
    y: int

p = Point(x=1, y=2)     # construct: every field, by name, exactly once
p.x = 10                # field assignment
d = p.x + p.y           # field access
```

- Fields are declared in an indented block; each needs a type annotation.
- Structs may reference structs defined later (forward references) and may
  contain arrays/other structs. A struct **cannot contain itself**, directly
  or indirectly.
- Construction requires all fields, by name, in any order; types must match.
- Struct and function names share one namespace.

## Strings

```Aoxn
s = "hello" + ", " + "world"   # concatenation (runtime malloc + memcpy)
print(s)                       # hello, world
print(len(s))                  # 12 (bytes)
print("abc" == "abc")          # true
print("apple" < "banana")      # true (byte-wise lexicographic)
```

- `+` concatenates two strings (only strings can be concatenated).
- All six comparison operators work on `(string, string)` via byte-wise
  `strcmp` semantics; `len(s)` is the byte length.
- Strings are **immutable**. Literals live in static storage; concatenation
  results are heap-allocated and intentionally not freed (no GC yet 鈥?safe
  because strings never mutate, so sharing/aliasing is sound).
- Strings may appear in struct fields, array elements, parameters, and
  returns; copies share the underlying bytes (safe: immutability).
- No string indexing yet (returns no `char` type 鈥?roadmap).

## Loops

```Aoxn
while cond:
    ...

for i in range(n):            # 0, 1, ..., n-1
for i in range(a, b):         # a .. b-1
for i in range(a, b, step):   # step may be negative (step != 0)
for x in arr:                 # iterate array elements (each copied into x)
```

- `range(...)` takes 1..3 `int` arguments; start/end/step are evaluated once
  at loop entry (Python semantics). A zero step loops forever (undefined, not
  checked).
- `for x in arr` requires an array; the loop variable gets the element type.
  Iterating strings is not supported yet.
- The loop variable follows normal binding rules (first use declares; the
  type is fixed thereafter).
- `break` exits the innermost loop; `continue` jumps to the next iteration.
  Both are errors outside a loop.

## f-strings

```Aoxn
name = "Aoxn"
print(f"hello {name}, {1 + 2}, {True}")   # hello Aoxn, 3, true
print(f"braces: {{literal}}")             # braces: {literal}
```

- `{expr}` embeds any expression whose type is `int`, `float`, `bool`, or
  `string`; the expression may contain string literals and nested calls.
- `{{` and `}}` produce literal braces.
- f-strings desugar to string concatenation via the `str()` builtin:
  ints format as decimal, floats as `%f` (6 decimals), bools as
  `true`/`false`.

## Generic functions

```Aoxn
def sort[T, N](arr: [T; N]) -> [T; N]:
    result = arr
    for i in range(N):
        for j in range(N - 1 - i):
            if result[j] > result[j + 1]:
                t = result[j]
                result[j] = result[j + 1]
                result[j + 1] = t
    return result

sort([3, 1, 2])       # T = int,   N = 3
sort([1.5, 0.5])      # T = float, N = 2
sort(["b", "a"])      # T = string, N = 2
```

- Type parameters (`T`) and array-length parameters (`N`) are declared in
  `[...]` after the function name. At most one length parameter per function.
- Monomorphization: every distinct (type, length) combination at a call site
  produces a dedicated instance at compile time; arguments are unified
  against the declared parameter types (inferred 鈥?no explicit type
  arguments).
- The length parameter is a compile-time `int` constant inside the body
  (`range(N)`, `N - 1`).
- Operations on `T` are checked per instance: `sort` requires `<` on `T`
  (int, float, string work; structs do not).
- Generic functions cannot be `main` or `extern`.

## Value semantics (arrays and structs)

Assignment, parameter passing, and returns **copy** the whole value (lowered
to memcpy). There are no references or pointers yet:

```Aoxn
b = a          # b is an independent copy
b[0] = 99      # does not change a
data: [int; 50000] = [0] * 50000    # Python-style replication
```

## Program structure

A program is a list of `import`, `struct`, `def`, and `extern def`
declarations; execution starts at `main`.

```Aoxn
import * from "../stdlib/stdlib.ax"   # resolved relative to the importing file

extern def sqrt(x: float) -> float    # C runtime function (FFI), no body

def main() -> int:
    print(sqrt(2.0))                  # 1.414214
    return 0
```

- `extern def` declares a C-runtime function: no body, resolved at link time
  from the default libraries. Extern functions cannot be named `main`, cannot
  be generic, and may only use the scalar types (`int`, `float`, `bool`,
  `string`, `void`) - aggregate parameters by value would need a struct ABI
  the `extern` surface cannot spell.
- **Imports** (W1-S3 module forms) load another file and merge it into one
  namespace:
  ```Aoxn
  import * from "./util.ax"        # whole-module merge
  import { helper, Vec } from "./util.ax"   # named (M1 merges all names)
  import main_config from "./cfg.ax"        # default import
  ```
  Paths starting `./` or `../` resolve relative to the importing file, with
  extension completion (`.ax`/`.ts`/`.tsx`, `index.<ext>`); bare names are
  package imports resolved under `aox_modules/` (the `aoxn pkg` client
  lands in W2). Each file is included exactly once (canonical path);
  circular imports are compile errors. `Aoxn run main.ax` alone is enough
  — imports pull in dependencies. The bare legacy form `import "path"`
  was **removed** in W1-S3; write `import * from "path"`.
- Multiple entry files on the command line (`Aoxn build a.ax b.ax`) are also
  merged, with import resolution applied to each.

`main` returns `int` (process exit code) or `void` (exit 0).

## Bindings

```Aoxn
x = 5               # inferred: int
x: int = 10         # annotated (Python-style); annotation must match
x = x + 5           # re-assignment keeps the declared type
```

- The first binding declares the variable; its type is fixed from the
  initializer (or checked against the annotation).
- Re-assignment must keep the declared type. Re-declaring with a different
  type is an error. No shadowing within a function (parameters included).
- `print(1)` prints `1`; `print(True)` prints `true` / `false` (readable).

## Augmented assignment

The Python assignment operators are accepted on every assignment target:

```Aoxn
x = 5
x += 3          # x = x + 3
x -= 1          # x = x - 1
x *= 4          # x = x * 4
x /= 7          # x = x / 7
x %= 3          # x = x % 3

s = "ab"
s += "cd"       # s = s + "cd"

arr = [1, 2, 3]
arr[0] += 10    # arr[0] = arr[0] + 10

p = Point(x=1, y=2)
p.x += 1        # p.x = p.x + 1
```

- `+= -= *= /= %=` desugar at parse time to the plain assignment shown on the
  right, so they never change what typecheck or codegen see.
- The rules are exactly those of the expanded form: the operand type must
  match the target, `+=` on a string concatenates, and using an augmented
  form on a name that was never bound is an error (the expansion reads the
  name before writing it).
- `x //= n` is not an operator: `//` is a division, not an assignment. Write
  `x = x // n`.

## Functions

```Aoxn
def fib(n: int) -> int:
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)
```

- Parameters require type annotations; the return annotation is optional
  (defaults to `void`).
- Recursion and mutual recursion are allowed; order of definition does not
  matter.

## Statements

- `if cond:` / `elif cond:` / `else:` - conditions must be `bool`.
- `while cond:` - any `bool` condition.
- `for var in range(...)` / `for var in arr:` - see Loops above;
  `break` and `continue` apply to it as well.
- `return` / `return expr`.
- `pass` - explicit empty statement.
- assignment (`x = e`, `x: t = e`, `x += e`) and expression statements.

## Expressions

```ebbnf
or    := and ("or"  | "||") and)*
and   := eq  (("and" | "&&") eq)*
eq    := rel (("==" | "!=") rel)*
rel   := add (("<" | "<=" | ">" | ">=") add)*
add   := mul (("+" | "-") mul)*
mul   := unary (("*" | "/" | "//" | "%") unary)*
unary := ("not" | "!" | "-" | "+") unary | primary
primary := INT | FLOAT | STRING | True | False | FSTRING
         | IDENT ("(" args ")")? | "(" expr ")"
         | "[" expr ("," expr)* "]" | "[" "]"      # array literal / index
postfix := primary ("(" args ")" | "[" expr "]" | "." IDENT)*
```

- `and`/`or`/`not` and `&&`/`||`/`!` are synonyms. `and`/`or` short-circuit.
- `True`/`False` and `true`/`false` are synonyms.
- **Chained comparison** (Python): `a < b <= c` means `(a < b) and (b <= c)`
  and short-circuits like a plain `and`. Each link is type-checked on its
  own, so a chain mixing types fails at the offending link. The middle
  operands are evaluated twice by the desugaring; that is only observable if
  a middle operand is a call with side effects, in which case hoist it into a
  variable first.
- **Unary `+`** is the identity on a numeric operand, as in Python.
- **`//`** is Python's integer-division spelling. Aoxn's `/` already truncates
  on two `int`s, so `//` and `/` are the same operator there. **Deviation
  from Python:** both truncate toward zero, so `-7 // 2` is `-3`, whereas
  Python floors to `-4`. `//` requires two `int` operands (like `%`), while
  `/` accepts `int` or `float`.
- `[e] * n` is array replication; it is a parse-time form, not a general
  multiplication of arrays.

## Semantics rules (strict, AI-verifiable)

- Every `if` / `while` condition must be `bool`.
- A non-void function must return a value on **all** paths (`if/elif/else`
  where every branch returns satisfies this).
- No unreachable statements (rejected at compile time).
- A name may be declared only once; annotations on re-assignment must match.

## Builtins

- `print(expr)` 鈥?one `int`, `float`, `bool`, or `string` (no arrays/structs);
  prints a trailing newline. Floats print with `%f` (6 decimals).
- `len(x)` 鈥?array: static length; string: byte length. Returns `int`.
- `str(x)` 鈥?convert `int`/`float`/`bool`/`string` to `string`.
- `to_int(x)` / `to_float(x)` 鈥?explicit scalar conversions (truncating
  toward zero / widening; `bool` converts through 0/1). These are the only
  int/float mixing the language allows, and what the TS front end's
  numeric tower lowers through.

## Raw memory (unsafe, for the standard library and systems code)

Addresses are plain `int` (pointer-sized). These builtins are the escape
hatch that lets the standard library implement growable containers and file
IO in Aoxn itself; they perform no checks.

- `load_i64(addr) -> int`, `store_i64(addr, v: int)`
- `load_f64(addr) -> float`, `store_f64(addr, v: float)`
- `load_u8(base, off) -> int`, `store_u8(base, off, v: int)` 鈥?`base` may be
  an `int` address or a `string` (byte access into the string)
- `as_string(p: int) -> string`, `as_ptr(s: string) -> int` 鈥?pointer
  reinterpretation

## Platform query

- `target_os() -> string` 鈥?compile-time platform query. Returns one of
  `"windows"`, `"linux"`, `"macos"`, `"other"`, folded to a module-internal
  string constant by the compiler (it is **not** a runtime syscall). Both
  compilers (Rust and self-hosted) resolve it from the same target triple, so
  the value is stable within a build and identical between compilers on the
  same machine. Use it to branch on platform-specific code (e.g. skip the
  Windows-only `_setmode` call on POSIX).

The stdlib builds on these: `struct Vec` (growable 8-byte slots:
`vec_new`/`vec_push`/`vec_get`/`vec_set`/`vec_free` - write-back style,
`v = vec_push(v, x)`), byte buffers, `read_file`/`write_file`, and
`system(cmd)` for process spawning. The UI toolkit is an immediate-mode GUI
in three files (see `docs/ui.md`): `stdlib/ui.ax` (portable core) +
`stdlib/ui_draw.ax` (the platform-neutral widget layer) + one backend -
`stdlib/ui_win.ax` (Win32/GDI) or `stdlib/ui_x11.ax` (X11 + Xft, which also
serves macOS through XQuartz). A program picks its OS with one import line;
the widget layer names no platform symbol at all.

## Tooling contract (AI-native)

- `Aoxn build file.ax [-o out] [--O0|--O1|--O2|--O3]` — native executable
  (O3 default: the level selects the clang `-O` used to compile the generated C).
- `Aoxn run file.ax [-- args...]` — compile and run.
- `Aoxn c file.ax` — print the generated C text (v0.29.0: the C-emitting
  backend is the only backend; `Aoxn ir` is kept as a deprecated alias).
- Optimization levels: `--O3` (default), `--O2`, `--O1`, `--O0` select the
  clang `-O` level used to compile the generated C. The **C text itself is
  level-independent** - the level only picks compiler flags. `--O1` roughly
  halves compile time on large inputs and is recommended for iteration and
  compile-time-sensitive CI (inlining-heavy code is slower at runtime, loop
  code is unaffected).
- The C backend is the only backend (`--backend c` is accepted for
  compatibility). There is no IR, no pass pipeline, and no `nsw`/`nuw`
  refinement: `AOXN_PASSES`, `AOXN_DUMP_IR`, `AOXN_BACKEND` and `AOXN_CG_TRACE`
  are **dead** since v0.29.0 and silently ignored.
- `Aoxn run` and `Aoxn build` share one content-hash cache of the built
  executable: the key covers the content of the entry file *and all
  transitive imports*, plus the compiler binary, every codegen-affecting
  option (`--O*`, `--cpu`, `AOXN_CPU`, `-l`/`-L`, resolved clang path) and the
  output-relevant link flags. `target/cache` holds the entries
  (`AOXN_CACHE_DIR` to relocate, `AOXN_NO_CACHE=1` to disable, 64-entry
  approximate LRU). Re-running or re-building an unchanged program skips
  compile and link (`run` executes the cached exe; `build` copies it to the
  `-o` destination). Any source or option change is a miss; output is
  identical to a fresh compile either way.
- `--json` - diagnostics as `{"ok":false,"errors":[{"stage","line","col","message"}]}`.
- `AOXN_DUMP_C=1` - dump the generated C to stderr; `AOXN_TIME=1` - per-phase
  wall clock; `AOXN_TC_TRACE=1` - per-function typecheck markers;
  `AOXN_CLANG=<path>` - select the clang executable.

Diagnostics stages: `lex`, `parse`, `type`, `internal`, `link`, `io`.

## Platform support

| Tier | Platform | Status | Toolchain |
|------|----------|--------|-----------|
| **1** | Windows x86_64 | fully supported | MSVC Build Tools + clang (winget LLVM provides it) |
| **2** | Linux x86_64 | supported (CI-tested) | apt `clang` |
| **2** | macOS arm64 (Apple Silicon) | supported (CI-tested) | preinstalled Apple clang |

Cross-compilation, MinGW, and 32-bit targets are out of scope (see
`docs/platform-migration-plan.md`). Since v0.29.0 the compiler has no LLVM
dependency: codegen emits ISO C and clang compiles it (`AOXN_CLANG` or `PATH`
locates the driver), so no source changes are needed to move between Tier 1/2
platforms.

## Performance

Aoxn lowers to ISO C and compiles with clang O3 to native machine code —
measured at parity with `clang -O3` on identical algorithms (see
`examples/bench_*.ax`):
loop sum, array scan, struct copies, for-loop iteration, and string
build/compare all land within 卤15% of clang.

## Standard library

`stdlib/stdlib.ax` is written **in Aoxn itself** and compiled together with
the program: math (`abs/min/max/clamp/pow_i/gcd/lcm/isqrt/is_prime/hypot`
+ `sqrt`/`floor`/`ceil` via FFI), and generic search/sort/aggregation:
`sort`, `linear_search`, `binary_search`, `max_of`, `min_of`, `reverse`,
`sum_int`, `sum_float` 鈥?all parameterized by element type and length.

## Roadmap

Ordered by how much they cost the language's Python parity.

1. Remaining Python operators and forms: `**` (right-associative power),
   `in` / `not in` as operators (array membership, string substring), and
   `is` / `is not`.
2. Conditional expressions (`a if c else b`) and multiple assignment /
   unpacking (`a, b = f()`), which need tuple or multi-target support in the
   AST.
3. Default parameter values (`def f(x: int = 3)`).
4. Module qualification (`lib.sort(...)`), selective imports, package layout.
5. String indexing / iteration (needs a `char` type or substring slices);
   f-string format specifiers (`{x:.2f}`) and multi-line f-strings.
6. Memory: string interning or arena freeing (currently concatenation leaks).
7. Standard library expansion: containers, IO, crypto.
8. Top-level statements as an implicit `main` (module-script mode).
9. Larger Python features, each of which is a real language design (not just
   syntax): `None` and optional types, `try`/`except`, `with`, generators and
   `yield`, closures and decorators, classes, dict/set literals.

