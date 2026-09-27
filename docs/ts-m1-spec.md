# TS-M1 技术规范 — TypeScript 前端 → 现有编译管线

> 决策已定：方案 A（TS 前端产出与现有管线相同的 AST/IR，直通 LLVM）；
> 独立包管理（自有 manifest/lockfile/registry + 一次性 npm 导入工具）；
> 旧 `import` 架构随 W1 一次切换移除。本文是 W1 的实现规范。

## 0. 分层路线

| 切片 | 内容 | 状态 |
|------|------|------|
| **S0** | TS 词法器（全 token 集 + ASI 换行标记） | **已完成**（`src/ts/lexer.rs`，12 测试） |
| **S1** | TS 解析器：声明/表达式子集 → 现有 AST | **已完成**（`src/ts/parser.rs`，10 测试含 4 个端到端编译运行） |
| S2 | 类型层映射 + 降级（TS 类型 → 管线类型） | — |
| S3 | 模块系统（import/export 解析）+ **移除旧 `import`**，stdlib/selfhost 同批迁移 | — |
| S4 | 包管理客户端 + `aoxn pkg import` npm 桥 | — |
| S5 | 验收样本三件套 + CI 接入 | — |

S1 已覆盖（端到端验证）：`function`/`interface` 声明、`const`/`let`、
if/while/do-while/C 风格 for/for-of、break/continue/return、调用/成员/索引、
字面量与数组字面量、`console.log`→`print`、`x.length`→`len(x)`、对象字面量
→struct 字面量（需 interface 标注）、`i++`/复合赋值语句。`aoxn build foo.ts`
直接可用（`.ts`/`.tsx` 走 TS 前端，其余走 Aoxn 前端，汇入同一 AST）。

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
| `number` | `float`(f64) | JS 数字即 double；位运算按 JS int32 语义降级。**S1 例外**：`number` 标注降为 `int`(i64)，浮点字面量仅在无标注位置按 `float` 推断；f64 全量语义与数值塔统一在 S2 |
| `boolean` | `bool` | |
| `string` | `string` | 不可变字节串；UTF-8（JS 为 UTF-16，差异记入已知偏差） |
| `T[]`（定长用法） | `[T; N]` | M1 数组为定长值语义；`push/pop` 等动态操作属 M2 |
| `interface` | `struct` | 按结构检查（TS 结构化类型在 S2 校验层实现） |
| `class` | struct + 方法函数 | 无原型链；`this` 为隐式首参 |
| `function` | `def` | 闭包捕获属 M2；M1 禁止逃逸闭包 |
| 泛型 `<T>` | `def f[T, N]` | 复用现有单态化器 |
| `null`/`undefined` | — | M1 不引入可空值；`?` 参数用哨兵或报错提示 |
| 模板串 `${}` | `"lit" + str(expr)` 链 | 与现有 f-string 降级一致 |

## 3. 模块系统（替代旧 `import`）

```typescript
import { render, type Page } from "./renderer";   // 相对路径
import express from "express";                    // 包导入（走包管理）
export function handler(): void { ... }
export default handler;
```

- **解析顺序**：`./`/`../` → 相对文件（`.ts`/`.tsx`/`/index.ts` 补全）；
  裸标识符 → 包目录 `aox_modules/<pkg>` + `aoxn.json` 的
  `main`/`exports`/`types` 字段。
- **include-once 与循环检测**沿用现有 loader 语义；解析器实现于
  Rust `src/lib.rs` loader 与 `selfhost/load.ax` 同步迁移。
- **旧 `import "path"` 语法在 W1-S3 删除**（编译器、stdlib、selfhost
  同批迁移，不留双轨）。

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
