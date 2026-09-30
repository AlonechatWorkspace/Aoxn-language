# TS-M1 技术规范 — TypeScript 前端 → 现有编译管线

> 决策已定：方案 A（TS 前端产出与现有管线相同的 AST/IR，直通 LLVM）；
> 独立包管理（自有 manifest/lockfile/registry + 一次性 npm 导入工具）；
> 旧 `import` 架构随 W1 一次切换移除。本文是 W1 的实现规范。

## 0. 分层路线

| 切片 | 内容 | 状态 |
|------|------|------|
| **S0** | TS 词法器（全 token 集 + ASI 换行标记） | **已完成**（`src/ts/lexer.rs`，14 测试） |
| **S1** | TS 解析器：声明/表达式子集 → 现有 AST | **已完成**（`src/ts/parser.rs`，14 测试含 7 个端到端编译运行） |
| **S2a** | 泛型函数 `<T>`、数组类型 `T[]`/`Array<T>`（→ `[T; N]` + 合成长度参数，复用单态化器）、模板串插值 `${}`（→ `str()` 拼接链）、显式类型参数检测 | **已完成** |
| **S2b** | f64 数值塔、联合/收窄、`any`/可选参数/元组、`as`/`!`、多长度参数数组 | **已完成**（多长度参数数组除外——单态化器仍是单长度参数，见 §0.1） |
| **S3** | 模块系统（import/export 解析）+ **移除旧 `import`**，stdlib/selfhost 同批迁移 | **已完成**（固定点 IR/COFF 逐字节保持） |
| S4 | 包管理客户端 + `aoxn pkg import` npm 桥 | — |
| S5 | 验收样本三件套 + CI 接入 | — |

S1 已覆盖（端到端验证）：`function`/`interface` 声明、`const`/`let`、
if/while/do-while/C 风格 for/for-of、break/continue/return、调用/成员/索引、
字面量与数组字面量、`console.log`→`print`、`x.length`→`len(x)`、对象字面量
→struct 字面量（需 interface 标注）、`i++`/复合赋值语句。`aoxn build foo.ts`
直接可用（`.ts`/`.tsx` 走 TS 前端，其余走 Aoxn 前端，汇入同一 AST）。

S2a 已覆盖（端到端验证）：泛型函数 `function f<T>(...)`（类型参数按名引用，
调用点自动推断，复用现有单态化器）；数组类型 `T[]`/`Array<T>`（函数签名内
→ `[T; N]` + 合成长度参数 `N`，调用点按字面量长度实例化）；模板串插值
`` `a${x}b` `` → `"a" + str(x) + "b"` 拼接链（支持嵌套模板与任意表达式）。
**限制**：每函数仅一个数组长度参数（多数组参数独立长度属 S2b）；数组返回
需有数组参数钉住长度；显式类型参数 `f<number>(x)` 明确报错（推断即可，
避免与 `a < b > (c)` 比较链产生歧义误判）。

S2b 已覆盖（端到端验证）：
- **f64 数值塔**：`number` 统一为 double（`Type::Float`）；数字字面量全按
  JS number 生成 f64；索引/长度等 int 位置经 `Cast`/`to_int` 转换；混合
  算术自动形态化（字面量向另一侧宽度靠拢）。位运算（`& | ^ ~ << >> >>>`）
  与 `%` 按 JS int32/fmod 语义降级到 `__ts_*` 运行时助手（Aoxn 源码注入）；
  `console.log` 与模板插值按 JS 口径格式化数字（`__ts_num`：整数值打印
  整数形式，非整数去尾零）。
- **联合 + null 判别收窄**：`T | null | undefined` 擦除为 `T`，null 走
  类型哨兵（string→`""`、number→`0`、bool→false）；`x == null` /
  `x != null` 降级为哨兵比较。**M1 偏差**：哨兵值与 null 不可区分
  （`""`/`0`/false 同时是 null）。非空并集 `A | B` 保留首成员（运行时
  标签属 TS-M2）。
- **`any`/`unknown`**：64 位 int 盒子（`as` 收窄）；`never` → `void`。
- **可选参数**：`x?: T` / `x: T = e` 调用点省略时填哨兵/默认值
  （post-pass 补参，函数提升顺序无关）。
- **元组**：`[A, B]` 合成值结构体（字段 `_0`/`_1`），`t[0]` 读字段。
- **`as`/`!`**：`x as T` 生成检查过的标量转换（int/float/bool）；
  `x!` 非空断言为恒等（M1 无可空值）。

S3 已覆盖：`import * from "p"`（整模块合并）、`import { a, b } from "p"`、
`import d from "p"`、`import "./p"`（副作用合并）；`export function`/
`export interface` 透传为普通声明；`./`/`../` 相对解析带扩展补全
（`.ax`/`.ts`/`.tsx`、`index.<ext>`），裸名走 `aox_modules/`（W2 的
`aoxn pkg` 客户端接管完整解析）。**旧 `import "path"` 已删除**（报错
提示新写法）——stdlib/selfhost/examples/web 全部 `.ax` 与自举
`parser.ax`/`load.ax` 同批迁移（自举 loader 新增 `.`/`..` 路径归一化，
include-once/循环检测的 key 稳定）。自举固定点（IR+COFF 字节级）重跑
保持一致。

### 0.1 遗留（S2b 未完成项）

- **多数组长度参数**：`f(a: T[], b: U[])` 的独立长度仍受"每函数一个长度
  参数"限制（GENERIC_LEN 单哨兵 + `len_param: Option<String>`）。解除需要
  单态化器携带长度参数向量（unify/subst/mangle 全链），计划在 W1 收尾
  或 TS-M2 前落地。
- `import * as ns`（命名空间对象）、`export default`、`export { x }`
  re-export、顶层 `const`/`let` 语句（模块初始化）→ 后续切片。
- `typeof`/`instanceof`/`in`、类、闭包 → TS-M2。

TS-M2（对象引用语义/闭包/GC/async）在 M1 之后，见
[web-platform-plan.md](web-platform-plan.md) §1 里程碑。

## 1. TS-M1 语法子集（解析器覆盖清单）

**声明**：`import`/`export`（named/default/re-export）、`function`、
`const`/`let`、`class`（字段+方法+constructor，无装饰器）、`interface`、
`type` 别名（对象/联合的受限形式）、`enum`（数字枚举）、`declare`（忽略）。

**语句**：`if/else`、`for`（C 风格 + for-of 数组）、`while`、`do`、
`switch`、`try/catch/finally`、`throw`、`return`、`break`/`continue`、
块语句、表达式语句。`var` 不支持（报错，语义为块作用域 let）。

**表达式**：字面量（number/string/boolean/null 受限/模板串）、标识符、
成员访问 `a.b`/`a[b]`、调用、`new`、一元/二元/三元、赋值复合、箭头函数
（表达式体+块体）、`typeof`/`instanceof`/`in`、展开 `...`（数组字面量）。

**类型标注**：`number|string|boolean|void|any|unknown|never`、对象类型、
接口名、数组 `T[]`/`Array<T>`、元组（定长）、函数类型、字面量类型、
联合 `A | B`（M1 受限：仅判别联合收窄）、`?` 可选、泛型 `<T>`（单态化
复用现有 GENERIC 机制）、`as`/`!` 断言。

**不在 M1**（明确报错并给迁移提示）：async/await、装饰器、namespace、
`var`、BigInt、正则字面量、动态 `import()`、原型改写、`this` 别名技巧。

## 2. 语义映射（TS 构造 → 管线构造）

| TS | 管线 | 说明 |
|----|------|------|
| `number` | `float`(f64) | JS 数字即 double（S2b 起全量）；位运算按 JS int32 语义降级到 `__ts_*` 助手；`%` 是 fmod；`console.log` 数字按 JS 口径打印 |
| `boolean` | `bool` | |
| `string` | `string` | 不可变字节串；UTF-8（JS 为 UTF-16，差异记入已知偏差） |
| `T[]`（定长用法） | `[T; N]` | M1 数组为定长值语义；`push/pop` 等动态操作属 M2 |
| `interface` | `struct` | 按结构检查（TS 结构化类型在 S2 校验层实现） |
| `class` | struct + 方法函数 | 无原型链；`this` 为隐式首参 |
| `function` | `def` | 闭包捕获属 M2；M1 禁止逃逸闭包 |
| 泛型 `<T>` | `def f[T, N]` | 复用现有单态化器 |
| `null`/`undefined` | 类型哨兵 | `T | null` 擦除为 `T`；null 值=类型哨兵（`""`/`0`/false），`x == null` 降级哨兵比较；偏差：哨兵与 null 不可分（TS-M2 引入真可空） |
| 模板串 `${}` | `"lit" + str(expr)` 链 | 与现有 f-string 降级一致 |

## 3. 模块系统（替代旧 `import`）

```typescript
import { render, type Page } from "./renderer";   // 相对路径
import express from "express";                    // 包导入（走包管理）
export function handler(): void { ... }
export default handler;
```

- **解析顺序**（S3 已实现）：`./`/`../` → 相对文件（`.ax`/`.ts`/`.tsx`
  与 `index.<ext>` 补全）；裸标识符 → `aox_modules/<pkg>`（完整 manifest
  解析 `main`/`exports`/`types` 属 W2 的 `aoxn pkg`）。
- **include-once 与循环检测**沿用 loader 语义；Rust `src/lib.rs` 与
  `selfhost/load.ax` 已同批迁移（自举侧新增 `.`/`..` 归一化保持 key 稳定）。
- **旧 `import "path"` 语法已删除**（编译器、stdlib、selfhost、examples、
  web 同批迁移，不留双轨；自举 `parser.ax` 同样拒绝旧形态）。

## 4. 包管理（独立生态，A2）

| 项 | 设计 |
|----|------|
| manifest | `aoxn.json`：name/version/main/exports/types/dependencies |
| lockfile | `aoxn.lock`（内容寻址，含完整性哈希） |
| 包目录 | `aox_modules/`（扁平 + 版本后缀消解冲突） |
| registry | 自有服务（协议对齐 npm Registry API 子集） |
| **npm 桥** | `aoxn pkg import <npm-pkg>[@ver]`：拉 npm tarball → 转 aoxn.json → 发自有 registry/入包目录 → 记 lock。一次性迁移工具，不做透明代理 |

## 5. 验收样本（无真实项目期间的基线）

1. **REST API 服务**：路由分发、中间件函数、JSON 组装、错误处理。
2. **SSR 页面渲染**：HTML 拼装、CSS Modules 导入、循环渲染表格。
3. **泛型工具库**：泛型函数/接口/类 + 单元断言。

每个样本须在 M1 达成：`aoxn build sample.ts` 产出可执行文件、输出与
`tsc`+Node 参考实现逐字节一致（字符串/JSON 口径）。

## 6. 测试与回归门

- 现有 97 测试保持全绿（TS 前端为增量代码，不触碰现有 IR 路径）。
- TS 前端自测：词法 token 流精确比对（S0 起）、解析 AST dump 比对、
  样本编译输出比对。
- 自举固定点（IR/COFF 字节级）不受影响，直至 S3 迁移 selfhost 时重跑。
