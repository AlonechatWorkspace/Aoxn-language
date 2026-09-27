# 代码生成与 LLVM · Codegen and LLVM FFI

> **中文**：Aoxn 的代码生成层用一份手写的 LLVM-C FFI（`src/llvm.rs`）把"聚合即地址"的值模型、指针 + `memcpy` + sret 的聚合 ABI、字符串长度缓存与原始内存内建发射成 LLVM IR，再经 `default<O3>` 管线与 `LLVMTargetMachineEmitToFile` 产出目标文件——本页是这个后端全部不变量与踩坑记录的权威清单。
> **English**: Aoxn's codegen layer uses a hand-written LLVM-C FFI (`src/llvm.rs`) to emit an "aggregates are addresses" value model, a pointer + `memcpy` + sret aggregate ABI, string-length caching and raw-memory builtins into LLVM IR, then runs the `default<O3>` pipeline and `LLVMTargetMachineEmitToFile` to produce an object file — this page is the authoritative list of that backend's invariants and hard-won traps.

## 中文

### 为什么手写 LLVM-C FFI

Aoxn 的"零外部 crate"是硬约束（`Cargo.toml` 没有任何依赖），而这里还有一条更硬的平台事实：

- Windows 官方安装包只提供 C API —— `LLVM-C.lib` + `LLVM-C.dll`，外加少量 import library（`libclang`、
  `liblldb`、`LTO`、`Remarks`、`libomp`），**没有**按组件拆分的 LLVM 静态库。
- `inkwell` / `llvm-sys` 恰好需要那些静态库，所以它们在 Tier 1 平台根本装不起来；`CONTRIBUTING.md`
  把它写成房屋规则："do **not** introduce `inkwell` or `llvm-sys`"。
- 因此绑定是一份薄到没有逻辑的 `extern "C"` 声明表：用到哪个符号就声明哪个。它是照 LLVM 18 的 C API
  写的，在 23.1.0 上无需改动即可工作（本机 `clang --version` = `clang version 23.1.0`；
  `C:\Program Files\LLVM\lib\LLVM-C.lib` 与 `bin\LLVM-C.dll` 都存在）。
- 所有 IR 构造逻辑都在 [`src/codegen.rs`](../src/codegen.rs)，`src/llvm.rs` 只提供类型、声明与常量。

这样做换来一个明确的好处：新增 LLVM 能力 = 两处改动（`src/llvm.rs` 加声明、`src/codegen.rs` 使用它），
不涉及构建脚本、不需要匹配版本号的 build 依赖。

### `src/llvm.rs` 的组织方式

句柄全部是不透明指针，不试图给 LLVM 的内部结构建模（177 行的文件里没有一个 `struct` 定义）：

```rust
pub type LLVMContextRef = *mut ();
pub type LLVMModuleRef = *mut ();
pub type LLVMBuilderRef = *mut ();
pub type LLVMTypeRef = *mut ();
pub type LLVMValueRef = *mut ();
pub type LLVMBasicBlockRef = *mut ();
pub type LLVMTargetRef = *mut ();
pub type LLVMTargetMachineRef = *mut ();
pub type LLVMTargetDataRef = *mut ();
pub type LLVMBool = c_int;
```

声明按用途分成若干 `extern "C"` 块：上下文/模块/builder、类型、值/函数/基本块、指令构造、验证与打印、
目标与发射、New Pass Builder。常量单独集中在一起，并在注释里写明它们对应哪个 C 枚举：

| 常量 | 值 | 对应 |
|---|---|---|
| `CODEGEN_OBJECT_FILE` | 1 | `LLVMCodeGenFileType`（assembly = 0） |
| `CODEGEN_LEVEL_NONE` / `LESS` / `DEFAULT` / `AGGRESSIVE` | 0 / 1 / 2 / 3 | `LLVMCodeGenOptLevel` |
| `RELOC_DEFAULT` / `RELOC_PIC` | 0 / 2 | `LLVMRelocMode` |
| `CODE_MODEL_DEFAULT` | 0 | `LLVMCodeModel` |
| `INT_EQ` / `NE` / `SGT` / `SGE` / `SLT` / `SLE` | 32 / 33 / 38 / 39 / 40 / 41 | `ICmp` 谓词 |
| `REAL_OEQ` / `OGT` / `OGE` / `OLT` / `OLE` / `UNE` | 1 / 2 / 3 / 4 / 5 / 6 | `FCmp` 谓词 |
| `VERIFY_RETURN_STATUS` | 0 | `LLVMVerifierFailureAction` |

两条带注释的历史教训直接写在声明旁边：

- 用 `LLVMCreateBuilderInContext`，**不要**用 `LLVMBuilderCreate`：18.1.8 安装包的 `LLVM-C.lib`
  缺后一个符号，context 版本在所有版本上都可用。
- `LLVMSizeOf` 的声明上挂着注释：它返回 `ptrtoint` 常量表达式，正因如此 codegen 更愿意用
  `LLVMStoreSizeOfType`。

### 新增 LLVM 能力前：先用 `findstr` 验证符号

规则很简单：**先证明符号存在于已安装的库里，再写 `extern "C"`。**

```powershell
# 命中：符号存在；未命中：退出码 1（不要写进 llvm.rs）
findstr /c:"LLVMRunPasses" "C:\Program Files\LLVM\lib\LLVM-C.lib"
```

本次会话实测过这条命令的两个方向：`LLVMRunPasses` 命中；故意编造的名字返回退出码 1。
`CONTRIBUTING.md` 的房屋规则用的是同一条命令形式（示例里的符号写成占位名 `LLVMFoo`），和
"不要引入 inkwell/llvm-sys"并列。

### 后端初始化与目标机

LLVM 的目标注册是**进程级全局**的，重复注册会让 `LLVMGetTargetFromTriple` 报
`Cannot choose between targets`，所以它被包在 `Once` 里：

```rust
/// LLVM target registration is process-global; registering twice makes
/// LLVMGetTargetFromTriple fail with "Cannot choose between targets".
static TARGET_INIT: std::sync::Once = std::sync::Once::new();

fn init_target() {
    TARGET_INIT.call_once(|| unsafe {
        LLVMInitializeX86TargetInfo();
        LLVMInitializeX86Target();
        LLVMInitializeX86TargetMC();
        LLVMInitializeX86AsmPrinter();
        LLVMInitializeAArch64TargetInfo();
        LLVMInitializeAArch64Target();
        LLVMInitializeAArch64TargetMC();
        LLVMInitializeAArch64AsmPrinter();
    });
}
```

X86 四件套里 `AsmPrinter` 不是可选项：漏了它，`LLVMTargetMachineEmitToFile` 会失败并报
`TargetMachine can't emit a file of this type`。AArch64 四件套是 v0.26 平台化时补的，与 X86 注册不冲突。

三元组是**动态取**的，不硬编码：`LLVMGetDefaultTargetTriple()` → 先 `LLVMSetTarget(module, triple)`，
再 `LLVMGetTargetFromTriple` 解析 target、建 target machine，最后 `LLVMDisposeMessage(triple)`。
本机（`cargo run -- ir examples\fib.ax`，默认 O3）真实输出的头部：

```llvm
; ModuleID = 'AOXN_module'
source_filename = "AOXN_module"
target datalayout = "e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-windows-msvc"
```

CPU 与 features：`cpu` 取自 `AOXN_CPU`（CLI 的 `--cpu native` 就是写这个环境变量），默认空串
（generic CPU）以保证产物跨机器可复现；`features` 是空串。reloc 模式按平台分支，见"平台化落点"。

**data layout 必须在发射任何 IR 之前建立**：`LLVMCreateTargetDataLayout(tm)` →
`LLVMSetModuleDataLayout`（`build_module` 的第一步，先于 C 入口包装与所有函数）。原因写在源码注释里：

- 聚合大小要折叠成普通整数常量，`memcpy` 的长度才能是字面量；
- `LLVMSizeOf` 会返回 `ptrtoint(gep)` 常量表达式，成千上万个这种常量会让 pass 管线与指令选择
  把时间都花在反复折叠它们上。

对应的实现是 `type_size`：查 `size_cache: HashMap<Type, LLVMValueRef>` 缓存，未命中则
`LLVMStoreSizeOfType(self.td, ty)`，得到一个普通 `i64` 常量（`td` 为空时防御性返回 0）。
真实 IR 里能看到折叠结果——数组字面量拷贝是 `i64 32`、结构体是 `i64 40`、`Vec2` 是 `i64 16`。

### 值模型：聚合在发射中的值一律是地址

`Type::is_compound()` 定义为数组或结构体（[`src/ast.rs`](../src/ast.rs)）。由此产生整套发射模型：

- **`emit_expr` 对任何复合类型返回指针，而不是 SSA 聚合值。** 变量返回它的 alloca 槽；索引/字段返回
  `getelementptr` 结果；数组/结构体字面量返回 entry 里 temp 的地址；返回聚合的函数调用返回它的 sret 出参。
- `emit_aggregate_ptr` 因此很短：`ArrayRep` 走 `fill_rep`；lvalue 走 `emit_lvalue`；其余先 `emit_expr`——
  复合类型直接转发那个地址（"there is nothing to materialize"），标量才落进 `val_temps`。
- **聚合的拷贝一律是显式 `memcpy`**：`copy_value(dst, src, t)` = `LLVMBuildMemCpy(builder, dst, 0, src,
  0, type_size(t))`。只要被拷贝的东西是聚合——绑定、赋值、参数传入被调方、返回值写出、数组字面量的
  元素、结构体字段、`[e] * N` 的每个元素、聚合实参——统统走这条路；标量仍然直接 `load`/`store`。
- **绝不整体 load/store 聚合。** 这是用血换来的：把 500 字节、40 字段的结构体当成一个 SSA 值 load
  一次，曾值约 15 秒 instcombine 加 14–44 秒指令选择（记录在仓库根的本地会话笔记 `AGENTS.md`，
  该文件未纳入版本控制）；v0.26 去掉它之后，自举编译器的 codegen 从 32.0s 降到 3.3s。

真实 IR（`cargo run -- ir examples\vectors.ax --O0`，节选；`--O0` 不跑任何 pass，所以这就是刚发射
出来的原始 IR）：

```llvm
%Vec2 = type { i64, i64 }

define void @add(ptr %0, ptr %1, ptr %2) {
entry:
  %lit2649489139192 = alloca %Vec2, align 8
  %param.a = alloca %Vec2, align 8
  call void @llvm.memcpy.p0.p0.i64(ptr %param.a, ptr %1, i64 16, i1 false)
  %param.b = alloca %Vec2, align 8
  call void @llvm.memcpy.p0.p0.i64(ptr %param.b, ptr %2, i64 16, i1 false)
  %field.ptr = getelementptr inbounds %Vec2, ptr %param.a, i32 0, i32 0
  %field = load i64, ptr %field.ptr, align 8
  …
  call void @llvm.memcpy.p0.p0.i64(ptr %0, ptr %lit2649489139192, i64 16, i1 false)
  ret void
}
```

注意三点：`ret` 的是 `void`；参数是被调方自己 `memcpy` 进局部槽的指针；`i64 16` 是折叠好的
常量而不是 `ptrtoint` 表达式。

### 聚合 ABI（v0.26）

聚合跨函数边界的方式被三条规则固定下来（源码注释与 `CONTRIBUTING.md` 都强调它是 ABI 敏感的）：

1. **聚合参数按指针传递**，被调方 `memcpy` 进自己的局部槽（值语义由此实现）。
   `abi_ty(t)` 对复合类型返回 `self.ptr`，其余返回 `ty_of(t)`。
2. **聚合返回用 sret 隐藏出参 + `ret void`**：声明函数时
   `let sret = !is_ext && f.ret.is_compound();`，并把 `self.ptr` 插入参数 0 位，返回类型改成
   `self.void`（用 `LLVMVoidTypeInContext`；给 `LLVMFunctionType` 传空类型引用是潜在崩溃）。
3. **`FnInfo { params, sret }` 携带调用点需要的信息**：`params` 是声明的参数类型表（调用点据此判断
   哪些实参要按地址传），`sret` 决定要不要先准备出参。`is_ext` 在调用点是用
   `info.entry_bb.is_null()` 判定的——`extern` 声明不建 entry 块。

被调方一侧：`sret_offset = if fn_sret { 1 } else { 0 }`，参数 `i` 取
`LLVMGetParam(fn_ref, i + sret_offset)`，复合类型 `copy_value(slot, arg, ty)`、标量 `store`。
返回语句里聚合返回值是 `LLVMGetParam(fn, 0)` 然后 `copy_value` + `ret void`。

调用点一侧：聚合实参先 `emit_aggregate_ptr` 拿地址再入参；sret 则先
`lit_temp(call_node_address, ty_of(ret_ty))` 拿到出参（entry 提升，名字形如 `lit<地址>`），
把调用结果直接写成"那个出参地址"——所以调用点不需要再拷贝一次。

真实 IR（`--O0` 转储，节选；`@add`/`@aoxn.main` 取自 `examples\vectors.ax`，`@rep` 取自本节后面
那个探针程序——两次转储，不是同一次）：

```llvm
define void @add(ptr %0, ptr %1, ptr %2) {   ; %0 = sret 出参，%1/%2 = 两个聚合参数
…
}

define void @rep(ptr %0) {                    ; 返回 [int; 3]：sret
…
  call void @llvm.memcpy.p0.p0.i64(ptr %0, ptr %lit1894141688024, i64 24, i1 false)
  ret void
}

define i64 @aoxn.main() {
entry:
  …
  call void @add(ptr %lit2649489153480, ptr %var.total, ptr %elem.ptr)
  call void @llvm.memcpy.p0.p0.i64(ptr %var.total, ptr %lit2649489153480, i64 16, i1 false)
  …
  call void @rep(ptr %lit1894141690416)
  call void @llvm.memcpy.p0.p0.i64(ptr %var.r, ptr %lit1894141690416, i64 24, i1 false)
```

为什么不用 by-value 聚合签名：那会强迫每个调用点构造、每个被调方提取整块 SSA 聚合值
（insertvalue/extractvalue 链），IR 体积在调用密集的代码里成倍增长，pass 管线与指令选择变得超线性
（自举编译器上曾约 5 倍 IR 膨胀、30 秒 codegen）。`extern def` 保持普通 C ABI（参数/返回都用
`ty_of`），因为那是 FFI 边界：实践中用到的 FFI 形状全是标量或指针（`int`/`float`/`bool`/`string`，
见 `examples\ffi_llvm.ax` 与自举驱动里的 LLVM-C 声明），聚合不经过 `abi_ty`。注意这是**约定而非
不变量**：前端只拒绝 extern `main` 与 extern 泛型，并不禁止 extern 写聚合参数/返回。

同一个坑的另一面记在自举移植记录里（v0.19）：by-value 结构体类型出现在 LLVM 函数签名里会让
Aoxn 自己写的 codegen 把 LLVM 挂住；指针 + sret 是唯一可行的形式。

### 两条命令式不变量

**一、allocas 必须插在 entry 块的顶部。** 必须用 `alloca_in_entry`，绝不"定位到 entry 末尾再 alloca"：

```rust
/// Allocas must live at the top of the function's entry block, even when
/// the entry block is already terminated (e.g. by short-circuit branches).
unsafe fn alloca_in_entry(&mut self, ty: LLVMTypeRef, name: &str) -> LLVMValueRef {
    let entry = self.fns[&self.cur_fn].entry_bb;
    let saved = self.cur_bb;
    let first = LLVMGetFirstInstruction(entry);
    if first.is_null() {
        LLVMPositionBuilderAtEnd(self.builder, entry);
    } else {
        LLVMPositionBuilderBefore(self.builder, first);
    }
    let n = self.cstr(name);
    let slot = LLVMBuildAlloca(self.builder, ty, n.as_ptr());
    self.pos(saved);
    slot
}
```

原因是第一条注释：发射 `let` 的时候，entry 块可能已经被短路分支（或 `if`）**终结**了——此时"在 entry
末尾插入"要么落在终结指令之后（模块非法），要么根本插不进去。变量槽、字面量 temp、replication 计数器、
字符串长度槽全部经由这个函数。

**二、`if` 的合并块必须惰性创建。** 两个分支都 `return` 时，预先创建的空合并块就是一个**未终结的
基本块**，`LLVMVerifyModule` 会直接报错。实现里 `end_bb` 初始为 null，只在某个分支没有终结时才
`add_bb("if.end")` 并跳过去；两个分支都终结时，函数干脆没有合并块。

### 临时缓存与地址稳定性

三个缓存让循环体只分配一次内存，键都是 **AST 节点地址**：

| 缓存 | 键 | 用途 |
|---|---|---|
| `lit_temps` | 字面量/复制/聚合调用节点的地址 | 数组与结构体字面量的 temp、sret 调用的出参 |
| `val_temps` | 表达式节点地址 | 非 lvalue 的标量物化（`f()[0]` 这类聚合基底先落地） |
| `iter_temps` | 复制节点地址 | `[e] * N` 的循环计数器 |

**绝不把克隆出来的 `Expr` 传给它们。** 临时对象的地址会被分配器回收，于是后一个位置的节点（可能
在另一个函数里）会复用到别人的 temp，LLVM 直接报
`Referring to an instruction in another function!`。源码里到处是这条纪律的痕迹：结构体字面量收集字段时
用 `fields.iter().map(|(n, e)| (n.clone(), e))` **借用**表达式，实参路径用 `&a.value`，注释明确写着
"borrow the field expression, never clone it"。

缓存效果在 IR 里可见：`lit_temp` 生成的 alloca 名字就是 `lit<十进制节点地址>`（例如
`%lit2649489139192 = alloca %Vec2`），`rep_iter`/`val_temp` 分别是 `rep.iter<key>` 与 `tmp<key>`；
它们全都出现在 `entry:` 的最上方，所以循环体反复执行不会再增长栈。

另外两条与地址/索引形状有关：

- **结构体字段 GEP 的索引必须是 `i32` 常量**（LangRef 规则），用 `i64` 会失败并报
  `Invalid indices for GEP pointer type`；数组索引是 `i64`。IR 里可以直接对照：
  `getelementptr inbounds %Vec2, ptr %param.a, i32 0, i32 0` 与
  `getelementptr inbounds [3 x %Vec2], ptr %var.path, i64 0, i64 %load10`。
- **字符串字面量是静态全局**：`string_lit` 用 `LLVMBuildGlobalStringPtr` 生成 `@.str<N>` 并按内容
  去重（`strings` 表），同时把 `s.len()` 记进 `str_lens`；`printf`/`snprintf` 的格式串走同一套机制，
  名字是 `@.fmt<N>`。真实输出：`@.str0 = private unnamed_addr constant [9 x i8] c"Profile[\00"`。

### 控制流发射

本页接下来（控制流、字符串与原始内存两节）引用的几段 IR 来自一个临时探针程序——它写在系统临时目录里、
不属于仓库，把短路、`[e] * N`、数组 `for`、原始内存内建、`str()` 与 f-string 放在同一个 `main` 里，
用 `cargo run -- ir <探针文件> --O0` 转储。**它只用来 dump IR，不要运行**：里面的 `store_u8(1024, …)`
是故意往硬编码地址写字节的原始内存示例，直接跑会访问违规（进程崩在写入那一刻，stdout 缓冲也会丢）：

```aoxn
struct Big:
    a: [int; 4]
    s: string

def pick(x: int, y: int) -> bool:
    return x > 0 and y > 0 or x == y

def rep() -> [int; 3]:
    return [7] * 3

def main() -> int:
    print(pick(1, 2))
    b = Big(a=[1, 2, 3, 4], s="hi")
    print(b.a[2])
    r = rep()
    print(len(r))
    total = 0
    for v in r:
        total = total + v
    print(total)
    print(load_u8("abc", 1))
    store_u8(1024, 0, 65)
    print(as_ptr("xyz"))
    print(str(3.5))
    print(str(True))
    print(f"n={3} s={True}")
    return 0
```

**短路 `and` / `or` 用 phi。** 左值算完后按语义条件跳转（`and`：假就去 end；`or`：真就去 end），
右值在 `sc.rhs` 里算，`sc.end` 用 `phi i1` 汇合；跳过路径的入值就是结果常量 `false`（`and`）/`true`
（`or`）。真实 IR（探针程序的 `--O0` 转储，节选）：

```llvm
  %load = load i64, ptr %param.x, align 8
  %tmp = icmp sgt i64 %load, 0
  br i1 %tmp, label %sc.rhs, label %sc.end

sc.rhs:                                           ; preds = %entry
  %load1 = load i64, ptr %param.y, align 8
  %tmp2 = icmp sgt i64 %load1, 0
  br label %sc.end

sc.end:                                           ; preds = %sc.rhs, %entry
  %sc.val = phi i1 [ false, %entry ], [ %tmp2, %sc.rhs ]
  br i1 %sc.val, label %sc.end4, label %sc.rhs3
…
  %sc.val8 = phi i1 [ true, %sc.end ], [ %tmp7, %sc.rhs3 ]
```

**C 入口包装在用户函数之前发射。** 先 `LLVMAddFunction(module, "main", i32())` 再声明用户函数，
LLVM 才会把这个包装命名为 `main`；用户自己的 `main` 被改名为 `aoxn.main`（源代码里的注释写作
`Aoxn.main`，实际发出的符号名是 `aoxn.main`，IR 里可直接看到）。包装体做三件事：

- Windows 上先 `_setmode(1, 32768)`（fd 1 = stdout，`_O_BINARY` = `0x8000`），保证 `\n` 不会被
  CRT 翻译成 `\r\n`；守卫是 Rust 侧的 `crate::platform::is_windows()`。
- 调 `aoxn.main`：返回 `int` 就 `trunc i64 → i32` 并 `ret` 它作为退出码；返回 `void` 或聚合
  （聚合走 `main.ret` 出参）就 `ret i32 0`。
- 真实 O3 输出：`%0 = tail call i32 @_setmode(i32 1, i32 32768)`、`%1 = tail call i64 @aoxn.main()`、
  `ret i32 0`。

**`while`** 是 `while.cond` / `while.body` / `while.end` 三段，`break` 跳 `end`、`continue` 跳 `cond`。

**`for` 的 range 与数组共用一条发射路径**（`emit_for`），区别只在迭代源与归纳槽：

- `range(n)` / `range(a, b)` / `range(a, b, step)` 在 entry 求值一次，缺省 `start = 0`、`step = 1`
  （常量）；range 循环用**循环变量本身**当归纳槽，并在进入 `for.cond` 前存入 start。
- 数组迭代先 `emit_aggregate_ptr` 求一次数组指针，元素类型与**静态长度**都在编译期确定；归纳槽是
  隐藏的 `i64` 索引 `for.idx<loop_count>`（初始 0），循环变量只是体内的一次拷贝：
  先 `for.idxv` 读索引、`for.elem` 取地址、`for.val` load，再 `store` 进变量槽。字符串数组还要为
  这次拷贝量一次长度写进长度槽。
- 条件统一是 `select (step > 0), (i < end), (i > end)`：真实 IR 里 range 的 step 是常量 1，
  所以折叠成 `%for.c = select i1 true, i1 %for.lt, i1 %for.gt`。
- `for.cont` 只递增归纳槽（`for.next = add nsw i64 %for.i6, 1`）然后回 `for.cond`；`break`/`continue`
  通过 `loop_stack` 上的 `(end_bb, cont_bb)` 对跳转（`continue` 因此会正确执行递增）。

真实 IR（数组迭代，节选）：

```llvm
  store i64 0, ptr %for.idx0, align 8
  br label %for.cond

for.cond:                                         ; preds = %for.cont, %print.end
  %for.i = load i64, ptr %for.idx0, align 8
  %for.lt = icmp slt i64 %for.i, 3
  %for.gt = icmp sgt i64 %for.i, 3
  %for.c = select i1 true, i1 %for.lt, i1 %for.gt
  br i1 %for.c, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %for.idxv = load i64, ptr %for.idx0, align 8
  %for.elem = getelementptr inbounds [3 x i64], ptr %var.r, i64 0, i64 %for.idxv
  %for.val = load i64, ptr %for.elem, align 8
  store i64 %for.val, ptr %var.v, align 8
```

**`[e] * N` 用运行时循环填充**（`fill_rep`），元素只求值一次，然后 `rep.cond/rep.body/rep.end` 三段
循环写入 temp；绝不展开成 N 个 store（同样的优化器爆炸问题）：

```llvm
rep.cond1894141688024:                            ; preds = %rep.body1894141688024, %entry
  %rep.i = load i64, ptr %rep.iter1894141688024, align 8
  %rep.c = icmp slt i64 %rep.i, 3
  br i1 %rep.c, label %rep.body1894141688024, label %rep.end1894141688024

rep.body1894141688024:                            ; preds = %rep.cond1894141688024
  %rep.elem = getelementptr inbounds [3 x i64], ptr %lit1894141688024, i64 0, i64 %rep.i1
  store i64 7, ptr %rep.elem, align 8
  %rep.next = add nsw i64 %rep.i1, 1
```

语句块发射还有一个由类型检查兜底的细节：`emit_block_into` 每发一条语句前检查 `terminated()`，
已经终结就停——因为类型检查已经拒绝了不可达代码。

### 字符串、内建与运行时

**拼接**是 `malloc(len1 + len2 + 1)` + 两次 `memcpy` + 写 NUL（`emit_str_concat`）；操作数长度来自
`str_len_of`，所以**结果长度已知并记入 `str_lens`**。结果按设计**永不释放**（字符串不可变、还没有 GC，
这是文档化行为）。真实验证（`examples/strings.ax` 的 `--O0` 转储，节选）：

```llvm
define ptr @title(ptr %0) {
entry:
  %param.p = alloca %Profile, align 8
  call void @llvm.memcpy.p0.p0.i64(ptr %param.p, ptr %0, i64 16, i1 false)
  %field.ptr = getelementptr inbounds %Profile, ptr %param.p, i32 0, i32 0
  %field = load ptr, ptr %field.ptr, align 8
  %str.len = call i64 @strlen(ptr %field)      ; 字段长度未知 → strlen
  %str.sum = add i64 8, %str.len               ; 左操作数是字面量 "Profile["，长度 8 已折叠
  %str.cap = add i64 %str.sum, 1
  %str.buf = call ptr @malloc(i64 %str.cap)
  call void @llvm.memcpy.p0.p0.i64(ptr %str.buf, ptr @.str0, i64 8, i1 false)
  %str.mid = getelementptr inbounds i8, ptr %str.buf, i64 8
  call void @llvm.memcpy.p0.p0.i64(ptr %str.mid, ptr %field, i64 %str.len, i1 false)
  %str.end = getelementptr inbounds i8, ptr %str.buf, i64 %str.sum
  store i8 0, ptr %str.end, align 1
  %str.sum1 = add i64 %str.sum, 1              ; 第二次拼接复用上一次的结果长度，不再 strlen
```

**字符串长度缓存**（`str_lens` / `len_slots`）是这一层的性能核心：

- `str_lens: 值 → 已知字节长度`，来源是字面量、拼接结果、`str()` 结果、`as_string()` 结果。
- `len_slots: 字符串变量的槽 → 一个 i64 alloca`，在该变量的 `let`、赋值、`for` 元素拷贝处更新。
- `str_len_of` 的顺序是：变量先读它的长度槽 → 查 `str_lens` → 都不行才退化成 `strlen`。
- 这就是 `s = s + piece` 循环保持 **O(总字节数)** 而不是 O(n²) 的原因。
- 真实 IR：`%str.len = load i64, ptr %str.len.slot`（`len(greeting)` 完全没有 `strlen` 调用）。

**不变量：任何原始 store 或非白名单 C 调用前必须失效缓存。** `invalidate_str_lens()` 直接清空
`str_lens` 与 `len_slots`（保守但安全），调用点有三处：`store_i64`/`store_f64`、`store_u8`，以及
`extern` 调用且 `!is_len_safe_extern(name)`。白名单就是永不改写"别人创建的字符串字节"的 C 助手：
`malloc`、`realloc`、`free`、`memset`、`strlen`、`strcmp`、`strncmp`、`snprintf`、`printf`、`puts`、
`_setmode`——它们自己创建的字符串在创建处就记录了长度。

**比较与排序**走 C 运行时 `strcmp`：六种比较全部映射到 `strcmp(l, r) <op> 0`，谓词取
`INT_EQ/NE/SLT/SLE/SGT/SGE`（真实验证：`%str.cmp = call i32 @strcmp(ptr @.str5, ptr @.str6)`、
`%str.bool = icmp slt i32 %str.cmp, 0`）。`len(str)` 是 `str_len_of`（缓存或 `strlen`）；
`len(array)` 是编译期常量（IR 里直接是 `i64 3`）。

**`str()` 与 f-string**：

- `int` → `malloc(32)` + `snprintf(buf, 32, "%lld", v)`；`float` → `malloc(64)` +
  `snprintf(buf, 64, "%f", v)`；返回的缓冲区把 `sext(snprintf 返回值)` 记进 `str_lens`，
  所以后续拼接不再 `strlen`（真实 IR：`%str.n64 = sext i32 %str.n to i64` → `%str.sum = add i64 …`）。
- `str(string)` 是恒等；`str(bool)` 用 **branch + phi** 指向静态全局 `"true"`/`"false"`。
  phi 结果**不在** `str_lens` 里，所以它的长度只能靠 `strlen`——真实 IR 里
  `print(f"n={3} s={True}")` 的最后一个拼接确实发出了 `%str.len = call i64 @strlen(ptr %str.bool21)`。
- f-string 在解析器里就脱糖成 `"lit" + str(expr) + …` 链，codegen 只看到普通拼接——所以上面那条
  `strlen` 回退就是 f-string 里 `str(bool)` 的真实代价。

**`print`**：`int`/`float`/`string` 分别是 `printf("%lld\n")`/`printf("%f\n")`/`printf("%s\n")`；
`bool` 走 branch + `"%s\n"` 打印可读的 `true`/`false`。格式串由 `fmt_lit` 去重成 `@.fmt<N>` 全局。
C 运行时函数（`malloc`/`strlen`/`strcmp`/`snprintf`/`printf`/`puts`/`_setmode`）按需惰性声明：
`get_extern` 先用 `LLVMGetNamedFunction` 找已声明的，找不到才 `LLVMAddFunction`，并缓存
`(函数值, 函数类型)`。

### 原始内存内建

这是自举编译器依赖的逃生舱，全部是 `inttoptr`/`ptrtoint` 加裸 `load`/`store`：

| 内建 | 参数 | 实现 |
|---|---|---|
| `load_i64(addr)` / `load_f64(addr)` | 地址必须是 `int` | `inttoptr` + `load i64` / `load double` |
| `store_i64(addr, v)` / `store_f64(addr, v)` | `(int, int\|float)` | `inttoptr` + `store`，随后失效长度缓存 |
| `load_u8(base, off)` | `base` 可 `int` 或 `string` | `string`：`i8` GEP 取址；`int`：`add` 后 `inttoptr`；`load i8` + `zext` 到 `i64` |
| `store_u8(base, off, v)` | `base` 可 `int` 或 `string` | 同样的寻址，`trunc` 到 `i8` 后 `store`，随后失效长度缓存 |
| `as_string(p)` | `p` 是 `int` | `inttoptr`；缓冲区此刻是完整的，所以量一次 `strlen` 并记入 `str_lens` |
| `as_ptr(s)` | `s` 是 `string` | `ptrtoint` 到 `i64` |

真实 IR（探针程序 `--O0`，节选）——注意字符串基址那条 GEP **没有** `inbounds`：

```llvm
  %mem.u8 = load i8, ptr getelementptr (i8, ptr @.str3, i64 1), align 1
  %mem.u8.ext = zext i8 %mem.u8 to i64
  store i8 65, ptr inttoptr (i64 1024 to ptr), align 1
  %7 = call i32 (ptr, ...) @printf(ptr @.fmt1, i64 ptrtoint (ptr @.str4 to i64))
```

两条形状上的规则必须记住：

- **普通聚合 GEP 是 `inbounds`，原始内存内建的 GEP 必须非 `inbounds`**
  （`LLVMBuildGEP2` 而不是 `LLVMBuildInBoundsGEP2`）——它们按设计接受任意地址，声明 `inbounds`
  等于对编译器撒谎，那是 UB。
- `int` 算术带 `nsw`（`LLVMBuildNSWAdd`/`NSWSub`/`NSWMul`/`NSWNeg`），因为规范说 `int` 溢出是 UB、
  越界索引是 UB，和 C 一样。`%` 是 `srem`、`/` 是 `sdiv`（不检查除零）。

类型检查对这套内建是严格对齐的（[`src/typecheck.rs`](../src/typecheck.rs)）：`load_u8` 恰好 2 个
位置参数、`store_u8` 恰好 3 个、`load_i64`/`load_f64` 恰好 1 个且地址必须是 `int`、`store_f64` 的值
必须是 `float`、`as_ptr` 只接受 `string`、`as_string` 只接受 `int`。codegen 侧的同类检查是防御性的，
正常路径上不会触发。

### 优化管线与目标文件发射

`build_module` 的顺序是固定的：目标机与 data layout → 声明结构体/函数 → 发射函数体 → 打印（可选）
→ 验证 → pass 管线。五个 `CgPhase` 计时器（`target`/`build`/`verify`/`passes`/`isel`）在
`AOXN_TIME=1` 时打印成 `cg.<名字>` 行。

优化级别到两套参数的映射：

| `--O*` | IR 管线（`pipeline_for`） | 后端级别（`codegen_level`） |
|---|---|---|
| `--O0` | 无（`None`，整段跳过） | `CODEGEN_LEVEL_NONE` = 0（fast-isel） |
| `--O1` | `default<O1>` | `CODEGEN_LEVEL_LESS` = 1 |
| `--O2` | `default<O2>` | `CODEGEN_LEVEL_DEFAULT` = 2 |
| `--O3`（默认） | `default<O3>` | `CODEGEN_LEVEL_AGGRESSIVE` = 3 |

`--O0` 必须把后端级别压到 0，源码注释解释了原因：只跳过 IR 管线而仍用级别 2，就等于付全价指令选择
却没有优化；级别 0 才真正走 fast-isel 快路。`AOXN_PASSES=<pipeline>` 在**任何级别 > 0** 时文本覆盖
管线（`std::env::var("AOXN_PASSES").unwrap_or(default_pipeline)`）；完整环境变量表见
[命令行与工具链](CLI-and-Tooling.md)。

管线本身是 New Pass Builder：`LLVMCreatePassBuilderOptions()` →
`LLVMRunPasses(module, passes, tm, opts)` → `LLVMDisposePassBuilderOptions(opts)`；返回非空错误串就
转成 `internal error: optimization pipeline failed: …`。

验证在管线**之前**：`LLVMVerifyModule(module, VERIFY_RETURN_STATUS, &mut msg)`；`AOXN_DUMP_IR=1`
时先 `LLVMPrintModuleToString` 把 **pre-verify** IR 打到 stderr。因为 `VERIFY_RETURN_STATUS` 不会改写
模块，而 `--O0` 不跑任何 pass，所以 `Aoxn ir --O0` 打印的就是刚发射完的原始 IR——这是读发射结构
（而不是读优化结果）的正确姿势。

目标文件由 `LLVMTargetMachineEmitToFile(tm, module, path, CODEGEN_OBJECT_FILE, &mut err)` 写出，
文件类型是 `1`（assembly 是 0）。两个 LLVM 23 上的坑写在本地会话笔记里、也体现在这段代码的形状上：

- **漏注册 `LLVMInitializeX86AsmPrinter`** → 发射时报 `TargetMachine can't emit a file of this type`。
- **`ErrorMessage` 出参必须非 NULL**（传 NULL 会让 LLVM 崩），所以这里总是传
  `&mut err`（`err` 先初始化为空指针），并在非零返回时把错误串转成 `internal` 诊断后
  `LLVMDisposeMessage`。

随后链接交给 clang（[`src/lib.rs`](../src/lib.rs) 的 `link_opts`，附带平台相关的 `/STACK` 与 POSIX
`-rpath`）；codegen 只负责目标文件，这一层与 CLI 的构建缓存（v0.26.3 起 `run`/`build` 共用内容哈希键，
未改动的重建直接复制缓存产物）是分开的阶段。v0.26.3 本身没有任何 codegen 变更，自举固定点
（IR + COFF 逐字节比对）原样通过。

与优化级别相关的测试（[`tests/pipeline.rs`](../tests/pipeline.rs)）：

- `optimization_levels_agree_on_program_output`：递归 + 循环 + 字符串的程序在 O0–O3 下 stdout
  必须一致（`45\n144\n01234\n`）——正确性不许依赖优化级别。
- `optimization_levels_produce_distinct_ir`：`O0 ≠ O3`、`O1 ≠ O3`、`O1 ≠ O2`，确认级别真的改变 IR。

### 平台化落点

codegen 里与平台相关的决策只有三处，全部由 [`src/platform.rs`](../src/platform.rs) 提供判据：

1. **重定位模式**：`if crate::platform::is_windows() { RELOC_DEFAULT } else { RELOC_PIC }`。
   Linux/macOS 走 PIC（现代发行版默认 PIE，非 PIC 目标文件链不进 PIE 可执行文件）；Windows COFF
   用默认模式。
2. **`target_os()` 内建的折叠**：codegen 直接
   `self.string_lit(crate::platform::target_os_name())`，把 `"windows" | "linux" | "macos" | "other"`
   变成一个指向静态全局的 `ptr`（类型检查要求它无参数、返回 `string`）。**两个编译器必须折叠出同一个
   值**——Rust 侧是 `platform::target_os_name()`，Aoxn 侧是 `selfhost/codegen.ax` 里的 `target_os()`；
   否则自举固定点（`selfhost_driver_self_compiles` 比较 stage-1 与 stage-2 的 IR 和 COFF 目标文件）
   立刻破坏。平台相关的语言代码因此可以这样写：`if target_os() == "windows": …`。
3. **`_setmode` 的守卫**：只有 `crate::platform::is_windows()` 为真时，C 入口包装里才会出现
   `_setmode(1, 0x8000)`；自举侧用 `target_os() == "windows"` 表达同一件事。

`platform::obj_ext()`（`.obj` / `.o`）与 `platform::exe_ext()` 由 `lib.rs` 在命名目标文件和可执行
文件时使用；`platform::llvm_link_name()`（`LLVM-C` / `libLLVM-<N>` / `libLLVM` 探测）和 POSIX 的
`-Wl,-rpath` 属于链接阶段，详见 [平台支持](Platform-Support.md)。

### 改 codegen 前必读清单

1. **先分类**：这段代码产出的是 SSA 值还是地址？聚合必须是地址——新分支若想 `load` 整个结构体/数组，
   先回去读"值模型"一节。
2. **拷贝用 `copy_value`**（`memcpy` + `type_size` 常量），绑定、赋值、传参、返回、字面量元素、字段
   初始化一处都不能漏；聚合大小永远来自 `LLVMStoreSizeOfType`，不要用 `LLVMSizeOf`。
3. **新 alloca 一律 `alloca_in_entry`**，绝不定位到 entry 末尾；temp 一律走 `lit_temps`/`val_temps`/
   `iter_temps`，循环体里不新增 alloca。
4. **新基本块要惰性创建**，跳过去之前用 `terminated()` 判断；两个分支都终结时不要造空合并块。
5. **别 clone 表达式给 temp 缓存**：传 `&expr`/`&a.value`；否则会出现
   `Referring to an instruction in another function!`。GEP 索引形状：字段 `i32` 常量、数组 `i64`。
6. **碰字符串路径时**：新写字节或调用非白名单 C 函数前调 `invalidate_str_lens()`；新造的字符串如果
   长度已知，就记进 `str_lens`（或更新 `len_slots`），否则 `s = s + piece` 会退化成 O(n²)。
7. **碰 ABI 时同时改两侧**：聚合参数/返回的声明（`abi_ty` + sret 隐藏参数）、被调方提取、调用点入参
   与出参 temp，四处缺一不可；`extern def` 保持普通 C ABI。
8. **新 LLVM 符号先 `findstr` 验证**，再写进 `src/llvm.rs`；改完至少跑
   `cargo test --test pipeline optimization_levels_agree_on_program_output` 与
   `cargo test --test pipeline selfhost_driver_self_compiles`（Windows 上才有意义的固定点），
   完整的 97 个端到端测试才是真正的门槛。

---

## English

### Why a hand-written LLVM-C FFI

Aoxn's "zero external crates" rule (`Cargo.toml` has no dependencies at all) meets a harder platform fact:

- The official Windows installer ships the C API only — `LLVM-C.lib` + `LLVM-C.dll`, plus a few other
  import libraries (`libclang`, `liblldb`, `LTO`, `Remarks`, `libomp`) — and **not** the per-component
  LLVM static libraries.
- `inkwell` / `llvm-sys` need exactly those static libraries, so they cannot be built on the Tier 1
  platform; `CONTRIBUTING.md` states the house rule plainly: "do **not** introduce `inkwell` or
  `llvm-sys`".
- The bindings are therefore a thin, logic-free table of `extern "C"` declarations: declare whatever
  symbol you actually use. They were written against the LLVM 18 C API and work unchanged on 23.1.0
  (locally `clang --version` reports `clang version 23.1.0`, and both
  `C:\Program Files\LLVM\lib\LLVM-C.lib` and `bin\LLVM-C.dll` exist).
- All IR construction lives in [`src/codegen.rs`](../src/codegen.rs); `src/llvm.rs` only provides types,
  declarations and constants.

The payoff is a crisp rule for new features: a new LLVM capability is a two-file change (`src/llvm.rs`
gains a declaration, `src/codegen.rs` uses it) with no build-script work and no version-matched build
dependency.

### How `src/llvm.rs` is organized

Every handle is an opaque pointer; the file never models LLVM's internal structures (177 lines, not one
`struct` definition):

```rust
pub type LLVMContextRef = *mut ();
pub type LLVMModuleRef = *mut ();
pub type LLVMBuilderRef = *mut ();
pub type LLVMTypeRef = *mut ();
pub type LLVMValueRef = *mut ();
pub type LLVMBasicBlockRef = *mut ();
pub type LLVMTargetRef = *mut ();
pub type LLVMTargetMachineRef = *mut ();
pub type LLVMTargetDataRef = *mut ();
pub type LLVMBool = c_int;
```

Declarations are grouped into `extern "C"` blocks by purpose: context/module/builder, types,
values/functions/blocks, instruction building, verification and printing, target and emission, and the
New Pass Builder. Constants live in one region with comments naming the C enum they come from:

| Constant | Value | C enum |
|---|---|---|
| `CODEGEN_OBJECT_FILE` | 1 | `LLVMCodeGenFileType` (assembly = 0) |
| `CODEGEN_LEVEL_NONE` / `LESS` / `DEFAULT` / `AGGRESSIVE` | 0 / 1 / 2 / 3 | `LLVMCodeGenOptLevel` |
| `RELOC_DEFAULT` / `RELOC_PIC` | 0 / 2 | `LLVMRelocMode` |
| `CODE_MODEL_DEFAULT` | 0 | `LLVMCodeModel` |
| `INT_EQ` / `NE` / `SGT` / `SGE` / `SLT` / `SLE` | 32 / 33 / 38 / 39 / 40 / 41 | `ICmp` predicates |
| `REAL_OEQ` / `OGT` / `OGE` / `OLT` / `OLE` / `UNE` | 1 / 2 / 3 / 4 / 5 / 6 | `FCmp` predicates |
| `VERIFY_RETURN_STATUS` | 0 | `LLVMVerifierFailureAction` |

Two history lessons are recorded directly next to the declarations:

- Use `LLVMCreateBuilderInContext`, **not** `LLVMBuilderCreate`: the 18.1.8 installer's `LLVM-C.lib`
  was missing the latter symbol, while the context variant works on every version.
- The declaration of `LLVMSizeOf` carries a comment noting that it returns a `ptrtoint` constant
  expression — which is exactly why codegen prefers `LLVMStoreSizeOfType`.

### Verifying a new LLVM capability before declaring it

The rule is simple: **prove the symbol exists in the installed library before writing `extern "C"`.**

```powershell
# A hit means the symbol exists; a miss exits with code 1 (do not add it to llvm.rs)
findstr /c:"LLVMRunPasses" "C:\Program Files\LLVM\lib\LLVM-C.lib"
```

Both directions were exercised in this session: `LLVMRunPasses` hits, a deliberately invented name
returns exit code 1. `CONTRIBUTING.md` states the same command *form* as the house rule (its example
symbol is the placeholder `LLVMFoo`), alongside the ban on
`inkwell`/`llvm-sys`.

### Backend init, target and data layout

LLVM's target registration is process-global, and registering twice makes `LLVMGetTargetFromTriple` fail
with `Cannot choose between targets`, so it is wrapped in a `Once`:

```rust
/// LLVM target registration is process-global; registering twice makes
/// LLVMGetTargetFromTriple fail with "Cannot choose between targets".
static TARGET_INIT: std::sync::Once = std::sync::Once::new();

fn init_target() {
    TARGET_INIT.call_once(|| unsafe {
        LLVMInitializeX86TargetInfo();
        LLVMInitializeX86Target();
        LLVMInitializeX86TargetMC();
        LLVMInitializeX86AsmPrinter();
        LLVMInitializeAArch64TargetInfo();
        LLVMInitializeAArch64Target();
        LLVMInitializeAArch64TargetMC();
        LLVMInitializeAArch64AsmPrinter();
    });
}
```

`AsmPrinter` is not optional in the X86 quartet: without it `LLVMTargetMachineEmitToFile` fails with
`TargetMachine can't emit a file of this type`. The AArch64 quartet was added during the v0.26 platform
work; it does not conflict with X86.

The triple is fetched dynamically rather than hardcoded: `LLVMGetDefaultTargetTriple()` →
`LLVMSetTarget(module, triple)` first, then `LLVMGetTargetFromTriple` to resolve the target and create
the target machine, then `LLVMDisposeMessage(triple)`. Real header emitted on this machine
(`cargo run -- ir examples\fib.ax`, default O3):

```llvm
; ModuleID = 'AOXN_module'
source_filename = "AOXN_module"
target datalayout = "e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-windows-msvc"
```

CPU and features: `cpu` comes from `AOXN_CPU` (the CLI's `--cpu native` sets that variable), defaulting
to the empty string (generic CPU) so output stays reproducible across machines; `features` is empty.
The relocation mode is chosen per platform — see the platform section below.

**The data layout must be in place before any IR is emitted**: `LLVMCreateTargetDataLayout(tm)` →
`LLVMSetModuleDataLayout`, the very first step of `build_module`, before the C entry wrapper and every
function body. The source comment gives the reason:

- aggregate sizes must fold to plain integer constants so `memcpy` lengths are literals;
- `LLVMSizeOf` returns a `ptrtoint(gep)` constant expression, and thousands of those make the pass
  pipeline and instruction selection spend their time re-folding them.

The implementation is `type_size`: look in the `size_cache: HashMap<Type, LLVMValueRef>` cache, and on a
miss call `LLVMStoreSizeOfType(self.td, ty)` to get a plain `i64` constant (defensively returning 0 when
`td` is null). The folded constants are visible in real IR — `i64 32` for an array literal copy, `i64 40`
for a struct, `i64 16` for `Vec2`.

### Value model: aggregates are addresses

`Type::is_compound()` is defined as array or struct ([`src/ast.rs`](../src/ast.rs)). That single
definition drives the whole emission model:

- **`emit_expr` returns a pointer for any compound type, never an SSA aggregate value.** A variable
  returns its alloca slot; an index/field expression returns the `getelementptr` result; an array/struct
  literal returns the address of its entry-hoisted temp; an aggregate-returning call returns its sret
  out-pointer.
- `emit_aggregate_ptr` is therefore short: `ArrayRep` goes through `fill_rep`; lvalues go through
  `emit_lvalue`; anything else is emitted first — compound types just forward that address ("there is
  nothing to materialize"), while scalars are materialized into `val_temps`.
- **Every aggregate copy is an explicit `memcpy`**: `copy_value(dst, src, t)` is `LLVMBuildMemCpy(builder,
  dst, 0, src, 0, type_size(t))`. Whenever the copied thing is an aggregate — bindings, assignments,
  parameter copy-in, return write-out, array literal elements, struct fields, every element of `[e] * N`,
  aggregate arguments — it takes that path; scalars are still plain `load`/`store`.
- **Never load/store a whole aggregate.** This one was paid for: loading a 500-byte, 40-field struct as
  a single SSA value once cost roughly 15s of instcombine plus 14–44s of instruction selection
  (recorded in the repo-root local session notes `AGENTS.md`, which is not under version control);
  removing it in v0.26 brought the self-hosting compiler's codegen from 32.0s down to 3.3s.

Real IR (`cargo run -- ir examples\vectors.ax --O0`, excerpt; `--O0` runs no passes, so this is the raw
emission):

```llvm
%Vec2 = type { i64, i64 }

define void @add(ptr %0, ptr %1, ptr %2) {
entry:
  %lit2649489139192 = alloca %Vec2, align 8
  %param.a = alloca %Vec2, align 8
  call void @llvm.memcpy.p0.p0.i64(ptr %param.a, ptr %1, i64 16, i1 false)
  %param.b = alloca %Vec2, align 8
  call void @llvm.memcpy.p0.p0.i64(ptr %param.b, ptr %2, i64 16, i1 false)
  %field.ptr = getelementptr inbounds %Vec2, ptr %param.a, i32 0, i32 0
  %field = load i64, ptr %field.ptr, align 8
  …
  call void @llvm.memcpy.p0.p0.i64(ptr %0, ptr %lit2649489139192, i64 16, i1 false)
  ret void
}
```

Three things to notice: the return is `void`; the parameters are pointers that the callee copies into
its own slot; and `i64 16` is a folded constant rather than a `ptrtoint` expression.

### The aggregate ABI (v0.26)

Three rules pin down how aggregates cross function boundaries (both the source comments and
`CONTRIBUTING.md` stress that this is ABI-sensitive):

1. **Aggregate parameters are passed by pointer**, and the callee `memcpy`s them into its own local slot
   (this is what implements value semantics). `abi_ty(t)` returns `self.ptr` for compound types and
   `ty_of(t)` otherwise.
2. **Aggregate returns use an sret out-pointer plus `ret void`**: when declaring a function,
   `let sret = !is_ext && f.ret.is_compound();`, the hidden `self.ptr` is inserted at parameter 0, and
   the return type becomes `self.void` (use `LLVMVoidTypeInContext`; handing a null type ref to
   `LLVMFunctionType` is a latent crash).
3. **`FnInfo { params, sret }` carries what call sites need**: `params` is the declared parameter type
   list (call sites use it to decide which arguments go by address), and `sret` decides whether an
   out-pointer must be prepared first. At call sites `is_ext` is derived from `info.entry_bb.is_null()`
   — `extern` declarations never get an entry block.

On the callee side: `sret_offset = if fn_sret { 1 } else { 0 }`, parameter `i` is
`LLVMGetParam(fn_ref, i + sret_offset)`, and compound parameters are `copy_value(slot, arg, ty)` while
scalars are stored. A compound `return` copies into `LLVMGetParam(fn, 0)` and then emits `ret void`.

On the call-site side: aggregate arguments first go through `emit_aggregate_ptr` to obtain an address;
for sret, `lit_temp(call_node_address, ty_of(ret_ty))` provides the out-pointer (entry-hoisted, named
`lit<address>`), and the call's value *is* that out-pointer — so the call site never copies the result a
second time.

Real IR (`--O0` dumps, excerpts; `@add`/`@aoxn.main` come from `examples\vectors.ax`,
`@rep` from the probe program later in this section — two dumps, not one):

```llvm
define void @add(ptr %0, ptr %1, ptr %2) {   ; %0 = sret out-pointer, %1/%2 = aggregate params
…
}

define void @rep(ptr %0) {                    ; returns [int; 3]: sret
…
  call void @llvm.memcpy.p0.p0.i64(ptr %0, ptr %lit1894141688024, i64 24, i1 false)
  ret void
}

define i64 @aoxn.main() {
entry:
  …
  call void @add(ptr %lit2649489153480, ptr %var.total, ptr %elem.ptr)
  call void @llvm.memcpy.p0.p0.i64(ptr %var.total, ptr %lit2649489153480, i64 16, i1 false)
  …
  call void @rep(ptr %lit1894141690416)
  call void @llvm.memcpy.p0.p0.i64(ptr %var.r, ptr %lit1894141690416, i64 24, i1 false)
```

Why not by-value aggregate signatures: they force every call site to build and every callee to extract
whole SSA aggregates (insertvalue/extractvalue chains), which multiplies IR size in call-heavy code and
makes the pass pipeline and instruction selection superlinear (about 5x IR growth and 30s of codegen on
the self-hosting compiler). `extern def` keeps the plain C ABI (both parameters and return use `ty_of`),
because that is the FFI boundary: an `extern` shape is always a scalar or a pointer
(`int`/`float`/`bool`/`string`). Every FFI declaration in practice — `examples\ffi_llvm.ax` and the
LLVM-C declarations inside the self-hosted driver — is one of those, so aggregates never go through
`abi_ty` there.

The other side of the same trap is recorded in the self-hosting port notes (v0.19): by-value struct
types in LLVM function signatures hung LLVM in the Aoxn-written codegen; pointers plus sret are the only
workable form.

### Two imperative invariants

**One: allocas must be inserted at the top of the entry block.** Always use `alloca_in_entry`, never
"position at the end of entry and alloca":

```rust
/// Allocas must live at the top of the function's entry block, even when
/// the entry block is already terminated (e.g. by short-circuit branches).
unsafe fn alloca_in_entry(&mut self, ty: LLVMTypeRef, name: &str) -> LLVMValueRef {
    let entry = self.fns[&self.cur_fn].entry_bb;
    let saved = self.cur_bb;
    let first = LLVMGetFirstInstruction(entry);
    if first.is_null() {
        LLVMPositionBuilderAtEnd(self.builder, entry);
    } else {
        LLVMPositionBuilderBefore(self.builder, first);
    }
    let n = self.cstr(name);
    let slot = LLVMBuildAlloca(self.builder, ty, n.as_ptr());
    self.pos(saved);
    slot
}
```

The comment says why: when a `let` is emitted, the entry block may already be **terminated** by
short-circuit branches (or an `if`) — inserting "at the end of entry" would then place the alloca after
the terminator (an invalid module) or fail outright. Variable slots, literal temps, replication counters
and string length slots all go through this function.

**Two: the `if` merge block must be created lazily.** When both branches `return`, a pre-created empty
merge block is an **unterminated basic block** and `LLVMVerifyModule` rejects it. The implementation
starts with a null `end_bb` and only calls `add_bb("if.end")` (plus a branch) from a branch that did not
terminate; when both terminate, the function simply has no merge block.

### Temp caches and address stability

Three caches make a loop body allocate once, all keyed by **AST node address**:

| Cache | Key | Purpose |
|---|---|---|
| `lit_temps` | literal / replication / aggregate-call node address | array and struct literal temps; sret out-pointers |
| `val_temps` | expression node address | materializing non-lvalue scalars (an aggregate base like `f()[0]`) |
| `iter_temps` | replication node address | the `[e] * N` loop counter |

**Never hand a cloned `Expr` to these caches.** A temporary's address is recycled by the allocator, so a
later site (possibly in another function) reuses a foreign temp and LLVM reports
`Referring to an instruction in another function!`. The discipline shows everywhere in the source:
struct literal fields are collected with `fields.iter().map(|(n, e)| (n.clone(), e))` after **borrowing**
the expression, argument paths use `&a.value`, and a comment spells it out — "borrow the field
expression, never clone it".

The caches are visible in IR: a `lit_temp` alloca is literally named `lit<decimal node address>` (for
example `%lit2649489139192 = alloca %Vec2`), `rep_iter` and `val_temp` produce `rep.iter<key>` and
`tmp<key>`, and all of them appear at the very top of `entry:`, so iterating a loop never grows the
stack.

Two more shape rules concern addressing:

- **Struct field GEP indices must be `i32` constants** (a LangRef rule); `i64` fails with
  `Invalid indices for GEP pointer type`. Array indices are `i64`. Both shapes are directly comparable
  in the IR: `getelementptr inbounds %Vec2, ptr %param.a, i32 0, i32 0` versus
  `getelementptr inbounds [3 x %Vec2], ptr %var.path, i64 0, i64 %load10`.
- **String literals are static globals**: `string_lit` emits `@.str<N>` via `LLVMBuildGlobalStringPtr`,
  deduplicates by content (the `strings` map) and records `s.len()` in `str_lens`. `printf`/`snprintf`
  format strings go through the same machinery with the names `@.fmt<N>`. Real output:
  `@.str0 = private unnamed_addr constant [9 x i8] c"Profile[\00"`.

### Control-flow emission

Several IR excerpts below (in this section and in the strings/raw-memory ones) come from a temporary probe
program — written in the system temp directory, not part of the repo — that puts short-circuit logic,
`[e] * N`, array `for`, the raw memory builtins, `str()` and an f-string into a single `main`, dumped with
`cargo run -- ir <probe file> --O0`. **It is only meant for dumping IR, do not run it**: the
`store_u8(1024, …)` is a deliberate raw-memory example writing bytes to a hard-coded address, so
executing it trips an access violation (and the crash takes the buffered stdout with it):

```aoxn
struct Big:
    a: [int; 4]
    s: string

def pick(x: int, y: int) -> bool:
    return x > 0 and y > 0 or x == y

def rep() -> [int; 3]:
    return [7] * 3

def main() -> int:
    print(pick(1, 2))
    b = Big(a=[1, 2, 3, 4], s="hi")
    print(b.a[2])
    r = rep()
    print(len(r))
    total = 0
    for v in r:
        total = total + v
    print(total)
    print(load_u8("abc", 1))
    store_u8(1024, 0, 65)
    print(as_ptr("xyz"))
    print(str(3.5))
    print(str(True))
    print(f"n={3} s={True}")
    return 0
```

**Short-circuit `and` / `or` use phi nodes.** After the left operand is evaluated, control branches on
it (for `and`: false jumps to the end; for `or`: true jumps to the end), the right operand is evaluated
in `sc.rhs`, and `sc.end` merges with an `i1` phi whose skipped-path incoming value is the result
constant `false` (`and`) or `true` (`or`). Real IR (from the probe program's `--O0` dump, excerpt):

```llvm
  %load = load i64, ptr %param.x, align 8
  %tmp = icmp sgt i64 %load, 0
  br i1 %tmp, label %sc.rhs, label %sc.end

sc.rhs:                                           ; preds = %entry
  %load1 = load i64, ptr %param.y, align 8
  %tmp2 = icmp sgt i64 %load1, 0
  br label %sc.end

sc.end:                                           ; preds = %sc.rhs, %entry
  %sc.val = phi i1 [ false, %entry ], [ %tmp2, %sc.rhs ]
  br i1 %sc.val, label %sc.end4, label %sc.rhs3
…
  %sc.val8 = phi i1 [ true, %sc.end ], [ %tmp7, %sc.rhs3 ]
```

**The C entry wrapper is emitted before the user functions.** `LLVMAddFunction(module, "main", i32())`
comes first, so LLVM names that wrapper `main`; the user's own `main` is renamed to `aoxn.main` (the
source comment writes `Aoxn.main`, the emitted symbol is `aoxn.main`, visible in the IR). The wrapper
body does three things:

- On Windows it first calls `_setmode(1, 32768)` (fd 1 = stdout, `_O_BINARY` = `0x8000`) so the CRT does
  not translate `\n` into `\r\n`; the guard is the Rust-side `crate::platform::is_windows()`.
- It calls `aoxn.main`: an `int` return is truncated `i64 → i32` and returned as the exit code; a `void`
  or aggregate-returning `main` (aggregates go through the `main.ret` out-pointer) returns `i32 0`.
- Real O3 output: `%0 = tail call i32 @_setmode(i32 1, i32 32768)`,
  `%1 = tail call i64 @aoxn.main()`, `ret i32 0`.

**`while`** is the familiar `while.cond` / `while.body` / `while.end` triple, with `break` targeting
`end` and `continue` targeting `cond`.

**`for` shares one emission path for ranges and arrays** (`emit_for`); only the iteration source and the
induction slot differ:

- `range(n)` / `range(a, b)` / `range(a, b, step)` are evaluated once at entry, with defaults
  `start = 0` and `step = 1` (constants); a range loop uses **the loop variable itself** as the
  induction slot and stores the start value before jumping to `for.cond`.
- Array iteration calls `emit_aggregate_ptr` once for the array pointer; the element type and the
  **static length** are known at compile time. The induction slot is a hidden `i64` index
  `for.idx<loop_count>` (initialized to 0), and the loop variable is only a copy written in the body:
  `for.idxv` loads the index, `for.elem` computes the address, `for.val` loads the element, and a store
  writes it into the variable slot. For arrays of strings the copy's length is measured once into the
  length slot.
- The condition is uniformly `select (step > 0), (i < end), (i > end)`; in real IR a range's step is the
  constant 1, so it folds to `%for.c = select i1 true, i1 %for.lt, i1 %for.gt`.
- `for.cont` only increments the induction slot (`for.next = add nsw i64 %for.i6, 1`) and jumps back to
  `for.cond`; `break`/`continue` jump through the `(end_bb, cont_bb)` pair on `loop_stack` (which is why
  `continue` still performs the increment correctly).

Real IR (array iteration, excerpt):

```llvm
  store i64 0, ptr %for.idx0, align 8
  br label %for.cond

for.cond:                                         ; preds = %for.cont, %print.end
  %for.i = load i64, ptr %for.idx0, align 8
  %for.lt = icmp slt i64 %for.i, 3
  %for.gt = icmp sgt i64 %for.i, 3
  %for.c = select i1 true, i1 %for.lt, i1 %for.gt
  br i1 %for.c, label %for.body, label %for.end

for.body:                                         ; preds = %for.cond
  %for.idxv = load i64, ptr %for.idx0, align 8
  %for.elem = getelementptr inbounds [3 x i64], ptr %var.r, i64 0, i64 %for.idxv
  %for.val = load i64, ptr %for.elem, align 8
  store i64 %for.val, ptr %var.v, align 8
```

**`[e] * N` fills its temp with a runtime loop** (`fill_rep`): the element is evaluated once, then a
`rep.cond/rep.body/rep.end` triple writes it into the temp. It is never unrolled into N stores (the same
optimizer blow-up):

```llvm
rep.cond1894141688024:                            ; preds = %rep.body1894141688024, %entry
  %rep.i = load i64, ptr %rep.iter1894141688024, align 8
  %rep.c = icmp slt i64 %rep.i, 3
  br i1 %rep.c, label %rep.body1894141688024, label %rep.end1894141688024

rep.body1894141688024:                            ; preds = %rep.cond1894141688024
  %rep.elem = getelementptr inbounds [3 x i64], ptr %lit1894141688024, i64 0, i64 %rep.i1
  store i64 7, ptr %rep.elem, align 8
  %rep.next = add nsw i64 %rep.i1, 1
```

One more statement-level detail is backstopped by the type checker: `emit_block_into` checks
`terminated()` before every statement and stops early — because type checking has already rejected
unreachable code.

### Strings, builtins and the C runtime

**Concatenation** is `malloc(len1 + len2 + 1)` + two `memcpy`s + a NUL byte (`emit_str_concat`); the
operand lengths come from `str_len_of`, so the **result length is known** and is recorded in `str_lens`.
The result is **never freed** by design (immutable strings, no GC yet — documented behavior). Verified
in real IR (`examples/strings.ax` `--O0` dump, excerpt):

```llvm
define ptr @title(ptr %0) {
entry:
  %param.p = alloca %Profile, align 8
  call void @llvm.memcpy.p0.p0.i64(ptr %param.p, ptr %0, i64 16, i1 false)
  %field.ptr = getelementptr inbounds %Profile, ptr %param.p, i32 0, i32 0
  %field = load ptr, ptr %field.ptr, align 8
  %str.len = call i64 @strlen(ptr %field)      ; field length unknown -> strlen
  %str.sum = add i64 8, %str.len               ; left operand is the literal "Profile[", length 8 folded
  %str.cap = add i64 %str.sum, 1
  %str.buf = call ptr @malloc(i64 %str.cap)
  call void @llvm.memcpy.p0.p0.i64(ptr %str.buf, ptr @.str0, i64 8, i1 false)
  %str.mid = getelementptr inbounds i8, ptr %str.buf, i64 8
  call void @llvm.memcpy.p0.p0.i64(ptr %str.mid, ptr %field, i64 %str.len, i1 false)
  %str.end = getelementptr inbounds i8, ptr %str.buf, i64 %str.sum
  store i8 0, ptr %str.end, align 1
  %str.sum1 = add i64 %str.sum, 1              ; the second concat reuses the previous result length
```

**String length caching** (`str_lens` / `len_slots`) is the performance core of this layer:

- `str_lens: value → known byte length`, populated by literals, concat results, `str()` results and
  `as_string()` results.
- `len_slots: string variable slot → an i64 alloca`, updated at that variable's `let`, assignment and
  `for`-element copy.
- `str_len_of` checks in order: a variable reads its length slot → `str_lens` → fall back to `strlen`.
- This is what keeps `s = s + piece` loops **O(total bytes)** instead of O(n²).
- Real IR: `%str.len = load i64, ptr %str.len.slot` — `len(greeting)` emits no `strlen` call at all.

**Invariant: any raw store or non-whitelisted C call must invalidate the cache first.**
`invalidate_str_lens()` simply clears both `str_lens` and `len_slots` (conservative but safe), and it is
called from exactly three places: `store_i64`/`store_f64`, `store_u8`, and an `extern` call where
`!is_len_safe_extern(name)`. The whitelist is the set of C helpers that never rewrite the bytes of a
string they did not create: `malloc`, `realloc`, `free`, `memset`, `strlen`, `strcmp`, `strncmp`,
`snprintf`, `printf`, `puts`, `_setmode` — strings they create get their length recorded at creation.

**Comparison and ordering** go through the C runtime `strcmp`: all six comparison operators map to
`strcmp(l, r) <op> 0` with the `INT_EQ/NE/SLT/SLE/SGT/SGE` predicates (verified in real IR:
`%str.cmp = call i32 @strcmp(ptr @.str5, ptr @.str6)`, `%str.bool = icmp slt i32 %str.cmp, 0`).
`len(str)` is `str_len_of` (cache or `strlen`); `len(array)` is a compile-time constant (literally
`i64 3` in the IR).

**`str()` and f-strings**:

- `int` → `malloc(32)` + `snprintf(buf, 32, "%lld", v)`; `float` → `malloc(64)` +
  `snprintf(buf, 64, "%f", v)`. The buffer records `sext(snprintf's return value)` in `str_lens`, so a
  later concat needs no `strlen` (real IR: `%str.n64 = sext i32 %str.n to i64` feeding
  `%str.sum = add i64 …`).
- `str(string)` is the identity; `str(bool)` uses a **branch plus phi** over the static globals `"true"`
  and `"false"`. A phi result is **not** in `str_lens`, so its length falls back to `strlen` — in real IR
  the last concat of `print(f"n={3} s={True}")` really does emit
  `%str.len = call i64 @strlen(ptr %str.bool21)`.
- f-strings desugar in the parser into `"lit" + str(expr) + …` chains, so codegen only ever sees plain
  concatenation — which makes that `strlen` fallback the real cost of `str(bool)` inside an f-string.

**`print`**: `int`/`float`/`string` use `printf("%lld\n")`/`printf("%f\n")`/`printf("%s\n")`; `bool`
branches into `"%s\n"` so it prints readable `true`/`false`. Format strings are deduplicated into
`@.fmt<N>` globals by `fmt_lit`. The C runtime functions (`malloc`/`strlen`/`strcmp`/`snprintf`/
`printf`/`puts`/`_setmode`) are declared lazily: `get_extern` first looks for an existing declaration via
`LLVMGetNamedFunction` and only then calls `LLVMAddFunction`, caching the `(value, type)` pair.

### Raw memory builtins

These are the escape hatch the self-hosted compiler relies on, all built from `inttoptr`/`ptrtoint` plus
raw `load`/`store`:

| Builtin | Arguments | Implementation |
|---|---|---|
| `load_i64(addr)` / `load_f64(addr)` | address must be `int` | `inttoptr` + `load i64` / `load double` |
| `store_i64(addr, v)` / `store_f64(addr, v)` | `(int, int\|float)` | `inttoptr` + `store`, then invalidate the length cache |
| `load_u8(base, off)` | `base` is `int` or `string` | `string`: address via an `i8` GEP; `int`: `add` then `inttoptr`; `load i8` + `zext` to `i64` |
| `store_u8(base, off, v)` | `base` is `int` or `string` | the same addressing, `trunc` to `i8`, store, then invalidate the length cache |
| `as_string(p)` | `p` is `int` | `inttoptr`; the buffer is complete at this point, so `strlen` is measured once into `str_lens` |
| `as_ptr(s)` | `s` is `string` | `ptrtoint` to `i64` |

Real IR (probe program `--O0`, excerpt) — note that the string-based GEP has **no** `inbounds`:

```llvm
  %mem.u8 = load i8, ptr getelementptr (i8, ptr @.str3, i64 1), align 1
  %mem.u8.ext = zext i8 %mem.u8 to i64
  store i8 65, ptr inttoptr (i64 1024 to ptr), align 1
  %7 = call i32 (ptr, ...) @printf(ptr @.fmt1, i64 ptrtoint (ptr @.str4 to i64))
```

Two shape rules to remember:

- **Ordinary aggregate GEPs are `inbounds`; raw memory builtin GEPs must stay non-`inbounds`**
  (`LLVMBuildGEP2`, not `LLVMBuildInBoundsGEP2`) — they take arbitrary addresses by design, and claiming
  `inbounds` would be lying to the compiler, which is UB.
- `int` arithmetic carries `nsw` (`LLVMBuildNSWAdd`/`NSWSub`/`NSWMul`/`NSWNeg`) because the spec makes
  `int` overflow and out-of-bounds indexing UB, exactly like C. `%` is `srem` and `/` is `sdiv` (no
  division-by-zero check).

Type checking is strictly aligned with this set ([`src/typecheck.rs`](../src/typecheck.rs)): `load_u8`
takes exactly 2 positional arguments, `store_u8` exactly 3, `load_i64`/`load_f64` exactly 1 with an
`int` address, `store_f64`'s value must be `float`, `as_ptr` accepts only `string`, and `as_string` only
`int`. The equivalent checks in codegen are defensive and do not fire on the normal path.

### The optimization pipeline and object emission

`build_module` follows a fixed order: target machine and data layout → declare structs and functions →
emit function bodies → optional IR dump → verification → pass pipeline. Five `CgPhase` timers
(`target`/`build`/`verify`/`passes`/`isel`) print as `cg.<name>` lines when `AOXN_TIME=1`.

How the optimization levels map onto the two parameter sets:

| `--O*` | IR pipeline (`pipeline_for`) | Backend level (`codegen_level`) |
|---|---|---|
| `--O0` | none (`None`, skipped entirely) | `CODEGEN_LEVEL_NONE` = 0 (fast-isel) |
| `--O1` | `default<O1>` | `CODEGEN_LEVEL_LESS` = 1 |
| `--O2` | `default<O2>` | `CODEGEN_LEVEL_DEFAULT` = 2 |
| `--O3` (default) | `default<O3>` | `CODEGEN_LEVEL_AGGRESSIVE` = 3 |

`--O0` must push the backend level to 0, and the source comment explains why: skipping only the IR
pipeline while still using level 2 means paying full instruction selection with no optimization; level 0
is what actually takes the fast-isel path. `AOXN_PASSES=<pipeline>` textually overrides the pipeline at
**any level > 0** (`std::env::var("AOXN_PASSES").unwrap_or(default_pipeline)`); the complete environment
variable table is in [CLI and Tooling](CLI-and-Tooling.md).

The pipeline itself is the New Pass Builder: `LLVMCreatePassBuilderOptions()` →
`LLVMRunPasses(module, passes, tm, opts)` → `LLVMDisposePassBuilderOptions(opts)`; a non-null error
string becomes `internal error: optimization pipeline failed: …`.

Verification runs **before** the pipeline: `LLVMVerifyModule(module, VERIFY_RETURN_STATUS, &mut msg)`.
With `AOXN_DUMP_IR=1` the module is printed to stderr via `LLVMPrintModuleToString` first, so what you
read is the **pre-verify** IR. Since `VERIFY_RETURN_STATUS` does not rewrite the module and `--O0` runs
no passes, `Aoxn ir --O0` prints exactly the raw emission — which is the right way to read the emission
structure instead of the optimized result.

The object file is written by
`LLVMTargetMachineEmitToFile(tm, module, path, CODEGEN_OBJECT_FILE, &mut err)`, where the file type is
`1` (assembly is 0). Two LLVM 23 traps are recorded in the local session notes and reflected in the shape
of this code:

- **Forgetting to register `LLVMInitializeX86AsmPrinter`** makes emission fail with
  `TargetMachine can't emit a file of this type`.
- **The `ErrorMessage` out-parameter must be non-NULL** (LLVM crashes on NULL), so this code always
  passes `&mut err` (initialized to a null pointer) and turns a non-zero return into an `internal`
  diagnostic after `LLVMDisposeMessage`.

Linking is then handed to clang ([`src/lib.rs`](../src/lib.rs)'s `link_opts`, with the platform-specific
`/STACK` flag and the POSIX `-rpath`); codegen only produces the object file. That layer is a separate
stage from the CLI build cache (since v0.26.3 `run` and `build` share one content-hash key, so an
unchanged rebuild just copies the cached artifact). v0.26.3 itself contains no codegen changes, and the
self-hosting fixed point (IR + COFF byte-for-byte comparison) passes unchanged.

The relevant tests ([`tests/pipeline.rs`](../tests/pipeline.rs)):

- `optimization_levels_agree_on_program_output`: a program with recursion, loops and strings must
  produce identical stdout at O0–O3 (`45\n144\n01234\n`) — correctness may not depend on the level.
- `optimization_levels_produce_distinct_ir`: `O0 ≠ O3`, `O1 ≠ O3`, `O1 ≠ O2`, confirming the levels
  really change the IR.

### Platform adaptation points

Codegen makes exactly three platform-dependent decisions, and all three ask
[`src/platform.rs`](../src/platform.rs) for the answer:

1. **Relocation mode**: `if crate::platform::is_windows() { RELOC_DEFAULT } else { RELOC_PIC }`.
   Linux/macOS use PIC (modern distros link PIE executables by default, and non-PIC objects cannot go
   into them); Windows COFF uses the default mode.
2. **Folding the `target_os()` builtin**: codegen calls
   `self.string_lit(crate::platform::target_os_name())`, turning
   `"windows" | "linux" | "macos" | "other"` into a `ptr` to a static global (type checking requires no
   arguments and a `string` result). **Both compilers must fold the same value** — the Rust side uses
   `platform::target_os_name()` and the Aoxn side uses `target_os()` inside `selfhost/codegen.ax`;
   otherwise the self-hosting fixed point (`selfhost_driver_self_compiles` compares the stage-1 and
   stage-2 IR *and* COFF object) breaks immediately. That is what makes platform-specific language code
   like `if target_os() == "windows": …` safe.
3. **The `_setmode` guard**: only when `crate::platform::is_windows()` is true does the C entry wrapper
   contain `_setmode(1, 0x8000)`; the self-hosted side expresses the same thing as
   `target_os() == "windows"`.

`platform::obj_ext()` (`.obj` / `.o`) and `platform::exe_ext()` are used by `lib.rs` when naming the
object and executable files; `platform::llvm_link_name()` (probing `LLVM-C` / `libLLVM-<N>` /
`libLLVM`) and the POSIX `-Wl,-rpath` belong to the link stage and are covered in
[Platform Support](Platform-Support.md).

### Pre-flight checklist before changing codegen

1. **Classify first**: is this code producing an SSA value or an address? Aggregates must be addresses —
   if a new branch wants to `load` a whole struct or array, re-read the value-model section.
2. **Copy with `copy_value`** (`memcpy` + a constant `type_size`): bindings, assignments, parameter
   passing, returns, literal elements and field initialization all need it; aggregate sizes always come
   from `LLVMStoreSizeOfType`, never from `LLVMSizeOf`.
3. **Every new alloca goes through `alloca_in_entry`**, never "position at the end of entry"; temps go
   through `lit_temps`/`val_temps`/`iter_temps`, and loop bodies must not add allocas.
4. **Create new basic blocks lazily** and check `terminated()` before branching; when both branches
   terminate, do not create an empty merge block.
5. **Never clone an expression into a temp cache**: pass `&expr`/`&a.value`, or you will meet
   `Referring to an instruction in another function!`. GEP index shapes: `i32` constants for fields,
   `i64` for arrays.
6. **On string paths**: call `invalidate_str_lens()` before any raw write or non-whitelisted C call; if a
   newly built string has a known length, record it in `str_lens` (or update `len_slots`), otherwise
   `s = s + piece` degrades to O(n²).
7. **When touching the ABI, change all sides**: the declaration (`abi_ty` + the hidden sret parameter),
   the callee's extraction, and the call site's arguments and out-pointer temp — four places, none
   optional. `extern def` keeps the plain C ABI.
8. **Verify any new LLVM symbol with `findstr`** before adding it to `src/llvm.rs`; after the change, at
   minimum run `cargo test --test pipeline optimization_levels_agree_on_program_output` and
   `cargo test --test pipeline selfhost_driver_self_compiles` (the fixed point only means something on
   Windows), but the full 97-test end-to-end suite is the real gate.

---

## 源文件 / Source files

- [src/codegen.rs](../src/codegen.rs) — the whole backend: `init_target`, target/data-layout setup,
  `build_module`, expression/statement emission, the aggregate ABI, temp caches, string handling, raw
  memory builtins, the pass pipeline and object emission.
- [src/llvm.rs](../src/llvm.rs) — the hand-written LLVM-C FFI: opaque handle types, `extern "C"`
  declaration blocks, ICmp/FCmp predicates, codegen levels, relocation modes and
  `CODEGEN_OBJECT_FILE = 1`.
- [src/platform.rs](../src/platform.rs) — `is_windows()`, `target_os_name()`, `obj_ext()`, `exe_ext()`,
  `stack_link_flag()`, `llvm_link_name()` probing.
- [src/ast.rs](../src/ast.rs) — `Type::is_compound()` (array | struct), `is_lvalue()`, `is_printable()`.
- [src/typecheck.rs](../src/typecheck.rs) — the interface codegen depends on: `CheckOutput.call_map`,
  monomorphized `instances`, strict arity/type rules for the raw memory builtins and `target_os()`.
- [src/lib.rs](../src/lib.rs) — the `*_lvl` entry points (`generate_ir_text` / `generate_to_object` call
  sites), `obj_ext()` usage, and the clang link step with `/STACK` and POSIX `-rpath`.
- [src/main.rs](../src/main.rs) — `--O0/--O1/--O2/--O3` parsing and `--cpu` → `AOXN_CPU`.
- [tests/pipeline.rs](../tests/pipeline.rs) — `optimization_levels_agree_on_program_output`,
  `optimization_levels_produce_distinct_ir`, the aggregate/string/for-loop tests and the self-hosting
  fixed-point tests.
- [docs/spec.md](../docs/spec.md) — the UB statements (`nsw`, unchecked indexing), value semantics,
  strings, the raw memory builtins and the `target_os()` contract.
- [CONTRIBUTING.md](../CONTRIBUTING.md) — the "no inkwell/llvm-sys" rule, the `findstr` symbol check and
  the aggregate ABI paragraph.
- [CHANGELOG.md](../CHANGELOG.md) — the v0.26.3 entry (compile-speed work, no codegen changes, fixed
  point unchanged).
- [Cargo.toml](../Cargo.toml) — version baseline v0.26.3 and the empty dependency list.
- [examples/fib.ax](../examples/fib.ax), [examples/vectors.ax](../examples/vectors.ax),
  [examples/strings.ax](../examples/strings.ax) — the programs whose IR is quoted above.
