# 网页开发平台设计讨论（v0）

> 背景：前一版 benchmark 只证明了「Aoxn 能写网页、跑得快」，但对标
> pnpm+nodejs+nextjs 的真正目标是**开发者体验的等价迁移**——团队用
> TypeScript 编码习惯不变、现有代码无感迁移。本文按议题给出方案选项与
> 建议，标 **【决策点】** 的需要共同拍板；标 **【本次落地】** 的已经实现。

议题清单：
1. TypeScript 语法完整兼容（核心诉求）——**已决策：方案 A（TS 前端 → 现有编译管线）**
2. 移除现有 import 架构，换 TS 式模块系统
3. 网页包管理架构——**已决策：A2 独立包管理**
4. 服务端状态可观测【本次落地第一步】
5. 测试覆盖补齐 macOS / Linux【本次落地】
6. 性能指标体系与复杂场景压测（请求耗时 / 页面渲染耗时 / 编译时长）
7. 样式方案——**已决策：B1 完整兼容 CSS 打底 + Tailwind 工具链 + CSS Modules**

---

## 1. TypeScript 语法完整兼容

**目标**：现有 TypeScript 项目 `*.ts`/`*.tsx` 直接编译运行，开发者不改
习惯、不学新语法（Aoxn 的 Python 风格语法退居实现层，不再是用户面）。

### 方案选项

| 方案 | 描述 | 迁移体验 | 代价 |
|------|------|----------|------|
| **A. TS 前端 → 现有编译管线（推荐）** | 新增 TS 语法 lexer/parser，产出与现有 Aoxn 管线相同的 AST/IR，直通 LLVM | 语法无感；语义按 TS 对齐分期 | TS 类型系统（结构化类型、联合、收窄、any、可选、元组）要在 typecheck 层重建 |
| B. TS 语法 + Aoxn 严格语义 | 只借语法，语义保持 Aoxn 严格性（如 int/float 不混用） | 现有代码大概率跑不起来 | 小，但违背「无感迁移」 |
| C. 嵌入 JS/TS 运行时（QuickJS 类） | 解释执行 | 完整 | 背离 AOT 编译定位，性能目标失效 |

**已决策：A（TS 前端 → 现有编译管线）**。关键工程量在两处：

1. **类型系统**：TS 是结构化类型 + 联合/交叉/字面量类型 + 控制流收窄；
   现有 typecheck 是严格名义风格。需要为 TS 语义重建检查层（`any` 先
   按逃生舱处理、`strict` 模式逐步收紧）。
2. **运行时语义**：JS 对象是引用语义 + 原型链 + 闭包 + GC；现有运行时是
   值语义 + 无 GC（字符串拼接结果按设计泄漏）。对象图/闭包需要真正的
   堆管理——这是「完整兼容」里最大的一块，必须分期。

### 建议里程碑

| 里程碑 | 覆盖 | 验收 |
|--------|------|------|
| **TS-M1：语法+静态子集** | 标注类型、interface、enum、泛型、函数/箭头函数、let/const、模板字符串、class（字段+方法）、import/export（见 §2） | 团队真实项目中「无 async、无闭包逃逸」的模块可编译运行 |
| **TS-M2：JS 运行时语义** | 对象引用语义、闭包、数组/Map 常用库、GC（或区域分配起步） | 常见 Node 服务代码（Express 路由风格）可跑 |
| **TS-M3：全量** | async/Promise、装饰器、namespace、完整 lib.dom/node.d.ts 常用面 | 「完整兼容 TypeScript 语法」达标 |

> 需要提供 2–3 个**团队真实项目/模块**作为迁移验收样本（这是排期与
> 里程碑校准的输入）。

---

## 2. 移除现有 import 架构

现状：`import "相对路径"`（include-once、相对文件解析，见 `src/lib.rs`
load_program 与 `selfhost/load.ax`）。目标形态是 TS 模块：

```typescript
import { render, type Page } from "./renderer";
import express from "express";            // 包导入
export function handler(req: Request) { ... }
```

- 解析规则采用 **node_modules 式自底向上查找** + `package.json`
  `exports`/`main`/`types` 字段；相对路径 `./` `../` 照 TS 语义。
- 影响面：Rust 侧 loader、自举侧 `selfhost/load.ax`、`stdlib/` 全部改用
  新语法；旧 `import "path"` 语法**移除**（不是保留兼容——按要求移除）。
- 建议节奏【决策点 C，见 §8】：新模块系统与 TS-M1 同一里程碑落地，
  stdlib/selfhost 同批迁移，**一次切换不留双轨**（双轨维护成本高，且
  自举固定点测试会翻倍）。

---

## 3.【决策点 A】网页包管理架构：分离 vs 合并共用

「现有包管理架构」如果指团队今天用的 **npm/pnpm 生态**：

| 选项 | 内容 | 优势 | 劣势 |
|------|------|------|------|
| **A1. 合并共用（推荐）** | 直接兼容 npm registry + node_modules 布局 + pnpm-lock.yaml；现有依赖、私有源、缓存全复用 | 无感迁移的最大杠杆；不重造 semver/lockfile/镜像这些重资产 | 受 npm 生态约束；包内 `postinstall` 脚本等 npm 特性要不要支持需定义 |
| A2. 独立包管理 | 自有 registry/包格式/lockfile | 完全可控；可加 AI-native 包元数据 | 生态冷启动；团队现有依赖全部要重新发布，直接违背无感迁移 |
| A3. 混合 | 消费侧完全兼容 npm（A1），发布侧允许自有 scope/registry | 兼顾迁移与长期自主 | 两套发布语义要文档说清 |

**已决策：A2 独立包管理**（自有 registry / 包格式 / lockfile）。设计要点：

- **包格式**：自有 manifest（暂称 `aoxn.json`）+ 自有 lockfile；版本号
  沿用 semver（学习成本最低）。
- **registry**：自建 registry 服务（协议对齐 npm Registry API 的成熟
  子集，便于工具链复用），支持私有 scope 与鉴权。
- **解析**：TS 式 `import ... from "pkg"` 走独立解析器（自底向上查找
  manifest，不读 node_modules）。
- **npm 生态桥接**：独立管理与「现有依赖无感迁移」之间存在张力——现有
  npm 依赖需要**一次性导入工具**（把 package.json 依赖转为自有格式并
  发布到自有 registry）或注册表代理层。这是 A2 的关键配套，请确认接受
  该成本。
- 工程量评估：包管理器（解析/下载/缓存/lockfile/完整性校验）约 4–6 周，
  registry 服务与 npm 桥接工具另计。

---

## 4. 服务端状态可观测【本次落地第一步】

要求：服务端状态可观测。设计为四件套：

| 通道 | 内容 | 状态 |
|------|------|------|
| **metrics** | `/metrics` Prometheus 文本格式：请求总数/按路由、响应字节、连接数（活跃/累计）、404 数、每请求处理耗时（sum/max，纳秒） | **本次落地**（参考服务器已实现） |
| **health** | `/healthz` 存活探针 | 计划随 TS 栈一起 |
| **logs** | 结构化 JSON 访问日志（每请求一行：时间、路由、状态、字节、耗时） | 计划随 TS 栈一起 |
| **traces** | 请求级追踪（可选导出 OTLP） | 二期 |

指标口径按 RED（Rate/Error/Duration）组织，后续 TS 栈提供 middleware
钩子（类比 prom-client / morgan），业务代码可注册自定义 counter/histogram。

---

## 5. 测试覆盖补齐 macOS / Linux【本次落地】

上一版只有 Windows（本机）数据。本次新增
`.github/workflows/web-bench.yml`：**windows-latest / ubuntu-latest /
macos-latest** 三平台矩阵——

1. 构建 Aoxn 编译器与 web 服务器（`server_win.ax` / `server_posix.ax`）；
2. **功能验证**：三个路由的响应体与 Node 参考实现逐字节一致性、
   keep-alive、`/metrics` 可用性；
3. **参考基准**：oha 短压测（5s × 1 轮）写入 job summary。

注意：CI runner 噪声大（共享 CPU），数据用于**趋势与回归**，不作绝对值；
正式对比仍以专用机器全量 `web/loadtest/bench.mjs`（3 轮交错取中位数）为准。
如团队能提供 Linux / macOS 机器，我可以出同口径全量数据。

---

## 6. 性能指标体系（基准 v2 设计）

上一版只测了空页面，参考意义有限。v2 指标与场景设计：

### 三类核心指标

| 指标 | 口径 | 工具 |
|------|------|------|
| **请求耗时** | TTFB、完整响应 p50/p95/p99；冷启动首请求；keep-alive 与新建连接分开报 | oha / 混合负载生成器 |
| **页面实际渲染耗时** | 真实浏览器：导航 → FCP / LCP / TTI（与服务端 SSR 耗时分开报——SSR 只是服务器端生成 HTML 的时间） | Playwright + CDP tracing |
| **编译时长** | 小/中/大三档项目 × 冷/热(缓存)/增量；`aoxn build` vs `next build` vs `tsc --noEmit` | 计时脚本 |

### 复杂场景（回答「空页面参考意义有限」）

| 场景 | 内容 | 压测方式 |
|------|------|----------|
| S0 空页 | 现有基线 | 保留作对照 |
| S1 图文页 | 20 张图（缩略图+大图）、50KB CSS、字体文件 | 页面 + 静态资源混合流量 |
| S2 重资源页 | HTML5 `<video>` 大文件流式播放（**Range/部分内容**）、多 CSS、JS bundle | 并发播放场景 |
| S3 静态资源吞吐 | 图片/CSS/字体分发（缓存头、gzip/br 压缩、ETag） | 纯资源压测 |
| S4 混合压测 | 页面 SSR + API + 资源（70/20/10 流量配比） | 持续 5 分钟看稳定性/内存 |

配套能力：~~静态文件服务、`Range` 支持、缓存协商（ETag/Cache-Control）~~
—— **已落地（2026-10-02，W2）**：`web/http_buf.ax` + `web/serve.ax` 实现
启动时预载的静态文件表（请求期零磁盘访问）、ETag（`"size.hash"`，
Aoxn/Node 双侧逐字节一致的确定性内容哈希）、`If-None-Match` → 304、
单区间/后缀 `Range` → 206、多区间忽略、不可满足 → 416、
`Cache-Control: public, max-age=60`；`web/loadtest/parity.mjs` 对
Aoxn/Node 两台服务器做全量语义对比。文件名走固定名表，不拼路径——
目录穿越在构造上不可能。仍缺：可选 gzip/br。

---

## 7.【决策点 B】样式方案

问题：是否兼容 CSS？页面样式、图形样式如何实现？

| 选项 | 页面样式 | 图形样式（图表/绘图） | 迁移体验 |
|------|----------|----------------------|----------|
| **B1. 完整兼容 CSS（推荐打底）** | `.css` 文件为一等资源；构建管线打包/压缩/指纹；行内 `style` 支持 | SVG 资产直接用；复杂图形后续提供类型化绘图 API | 现有 CSS/样式代码直接用，符合无感迁移 |
| B2. CSS-in-TypeScript | styled-components/emotion 风格，样式即 TS 代码，类型安全 | 同左 | 适合新代码；存量 CSS 仍需 B1 |
| B3. 自有样式 DSL | 声明式样式语言 | 统一 DSL | 学习成本，违背无感迁移 |

**已决策：B1 完整兼容 CSS 打底，并兼容 Tailwind 类工具链与 CSS
Modules**。落地形态：

- **一期**：`.css` 为一等资源 + 构建管线（打包/压缩/指纹/缓存协商），
  现有样式资产零改动 —— **已落地（v0.34.0，见
  [`css-assets.md`](css-assets.md)）**：`import` 到的 `.css` 在
  `load_file` 被截获进资产管线（`src/assets.rs`），`@import` 就地内联、
  保守压缩（只去注释与冗余空白，不做选择器合并/重排）、指纹化，产物经
  `styles()` / `styles_fingerprint()` 作为编译期常量嵌入；
- **CSS Modules**：构建器实现 `*.module.css` 局部类名哈希导出 —— **已落地
  （v0.34.0）**：类名按文件路径哈希改写，经 `<stem>_class(name)` 取用；
  改写器识别选择器上下文，字符串字面量、`@media` 参数、声明值中的 `.`
  均不误改（各有测试钉住）；
- **Tailwind 类工具链**：**接入方式已定为「预生成产物」**（v0.34.0）。
  Tailwind 本体是纯 JS npm 包、无 `aoxn.json`，而 `aoxn npm-import` 按设计
  拒绝这类包（`plain_js_package_is_rejected`）——Aoxn 链接的是 Aoxn 包，
  不是 JavaScript。改为前端自行运行 Tailwind CLI，产物作为普通 `.css`
  进入同一管线：既不把 Node 变成构建前置（一键安装刻意避免），也与
  「clang 是外部前置」的边界一致。**JIT 扫描器未实现**——它要么把 Node
  变成前置，要么要求长期维护一个「Tailwind 兼容子集」的保真度；
- **二期**：CSS-in-TS 类型化封装（styled API）作为可选糖；
- **图形样式**：一期 SVG + CSS；复杂图形（Canvas 类）二期提供类型化
  绘图 API。

---

## 8. 里程碑与剩余待决清单

已决策：TS 前端方案 A、包管理 A2 独立、样式 B1 + Tailwind + CSS Modules。

| 阶段 | 内容 | 状态 |
|------|------|------|
| **W0** | 可观测性 `/metrics`（RED 指标）+ 功能测试 `parity.mjs`；三平台 CI（`web-bench.yml`）功能验证 + 参考基准；本设计文档 | **已完成** |
| **W1** | TS-M1 前端（语法子集→现有管线）+ 新模块系统（import/export）+ **移除旧 import**；stdlib/selfhost 同批迁移 | 待启动（依赖【C】） |
| **W2** | 独立包管理（manifest/lockfile/registry 客户端 + npm 桥接）；CSS 资源管线 + CSS Modules + Tailwind 接入；静态文件服务 + Range + 缓存头 | 部分落地：静态文件服务已完成（2026-10-02）；**CSS 管线 + CSS Modules + Tailwind 接入已完成（2026-10-03，v0.34.0 建管线；v0.36.0 补齐产物目录、`url()` 改写、`styles.title` 点号访问与内置 Tailwind 生成器）**；包管理客户端进行中（`aoxn-pkg` beta） |
| **W3** | 基准 v2 全量执行（复杂场景 + 浏览器渲染指标 + 编译时长），Linux/macOS 专用机数据 | 依赖 W2 |
| **W4** | TS-M2 运行时语义（对象/闭包/GC）→ 团队真实项目迁移验收 | 依赖样本项目 |

**剩余待决**（2026-09-27 已全部决策，执行者自行选定）：
- 【C】旧 `import` 移除节奏：**随 W1 一次切换，不留双轨**（双轨使自举
  固定点测试面翻倍）。
- 【A2 配套】npm 桥接：**一次性导入工具** `aoxn pkg import <pkg>`（拉取
  npm tarball → 转 aoxn.json → 入自有 registry/包目录并记入 aoxn.lock）；
  注册表代理层不做，留作远期选项。
- 【TS】TS-M1 验收样本：暂以 3 个规范样本（REST API 服务 / SSR 页面渲染 /
  泛型工具库）作为验收基线，团队真实项目到位后替换。
- 【渲染指标】**认可** Playwright + Chrome（FCP/LCP/TTI）。
- 【机器】CI 参考值即可满足趋势/回归；全量协议（3 轮交错）在本机执行。

技术规范见 [`docs/ts-m1-spec.md`](ts-m1-spec.md)。
