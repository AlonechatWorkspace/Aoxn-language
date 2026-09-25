# P2 优化方案:编译器自身性能与代码质量

> 状态:方案(未实施)。P0(正确性)与 P1(生成代码运行性能)见 CHANGELOG;
> 本文档描述下一步"让编译器跑得更快、代码更干净"的工作项。
> 每项都标注了位置、预期收益和风险,按建议顺序排列。

## 目标

1. **编译速度**:大型/泛型密集程序(如 stdlib + 自举前端)的 typecheck 与
   parse 耗时下降,目标 20–40%。
2. **代码质量**:消除样板与重复、收敛巨型函数、补齐健壮性,使后续
   (自举数组支持等)更易维护。
3. **可度量**:先建立编译耗时基线,每项改动前后对比。

## 度量方法(先做)

- 新增 `AOXN_TIME=1` 环境变量,在 `lib.rs` 各流水线阶段打印 wall-clock
  (lex / parse / typecheck / codegen / link 各自耗时,仿 `AOXN_TC_TRACE` 风格)。
- 基准输入:`stdlib.ax` 全量 + `selfhost/typecheck_demo.ax`、
  `examples/stdlib_demo.ax`,用 `cargo run -- build` 计时;连续 3 次取最优。

---

## A. 编译器热路径(性能)

### A1. 单态化残余克隆(P0 已修大头,收尾)
- 位置:`src/typecheck.rs`
  - `:905` 每个具体调用点 `self.sigs[name].clone()`(Vec<Type> 深拷贝)
    → 改为借用或只 clone 返回类型;
  - `:895` `layout.clone()` 只为躲 `self.structs` 借用 → 用 `Rc`/分离借用改写;
  - `:650`、`:1194` 字段类型 clone → 返回 `&Type`。
- 预期:泛型密集文件 typecheck 时间可感知下降;风险低。

### A2. `bump()` 不再克隆 Token
- 位置:`src/parser.rs:36` — `self.toks[self.idx].tok.clone()` 对每个消费的
  token 整体克隆(含 `String` 载荷、`FStr(Vec<Token>)`)。
- 方案:`std::mem::replace(&mut self.toks[i].tok, Tok::Eof)` 取走所有权,
  或让游标携带 `&Tok`、仅在需要拥有时 clone。
- 预期:parse 阶段分配量减半级别;风险低(注意 `peek`/`peek2` 仍返回引用)。

### A3. 变量查找改 Map(作用域栈)
- 位置:`src/typecheck.rs:1554` `lookup()` 逆序线性扫描
  `Vec<(String, Type)>`,每次命中还 clone `Type`;作用域只推不弹
  (`:339`、`:445`),查询复杂度 O(全部局部变量)。
- 方案:每函数一个 `HashMap<String, (Type, 声明深度)>` + 作用域深度计数
  (出块时惰性失效,或用 Vec<HashMap> 作用域栈);返回 `&Type`。
- 同类:`src/codegen.rs` 的 `struct_fields` 每次字段访问线性扫描
  (`:895`、`:974`、`:1289`)→ 换 `HashMap<String,(usize,Type)>`。
- 预期:大函数体 typecheck 显著加速;风险低。

### A4. f-string 插值的重复词法分析
- 位置:`src/lexer.rs:571` — 每个 `{...}` 插值重新收集 `Vec<char>`、
  补空格前缀后整段递归 `lex()`。
- 方案:对插值子串直接构造子 Lexer(共享 char 缓冲,避免重复 collect);
  或解析插值时复用主 lexer 的切片视图。
- 预期:f-string 重的源文件加速;风险中(布局/列号语义要保持,
  `tests/pipeline.rs` 有 f-string 错误位置测试)。

### A5. 杂项
- `src/codegen.rs:28/43` `call_map.clone()` → `&HashMap` 借用(`Gen` 加生命周期)。
- `src/parser.rs:465-474` elif 折叠 `c.clone()/b.clone()` → `mem::take`/移动。
- 词法层 `Vec<char>` 全量拷贝(`lexer.rs:83`)→ 保留,收益小于改动风险;
  仅在 A4 落地后顺带评估。

---

## B. 代码质量(可维护性)

### B1. `Diag` 构造辅助
- 100+ 处五字段字面量样板(仅 `Parser::unexpected` 有 helper)。
- 方案:`Diag::at(stage, pos, msg)` + `Tc::err(...)` 便捷方法;机械替换。
- 收益:每处错误信息改动的 diff 显著变小;为"多错误收集"铺路。

### B2. 拆分巨型函数
- `Tc::check_expr`(`typecheck.rs:561`,~516 行):builtins、调用、各运算符
  拆子方法;
- `Gen::emit_expr`(`codegen.rs:922`,~350 行):内置函数分发拆出
  `emit_builtin`;
- `lex`(`lexer.rs:81`,~385 行)按 token 类别拆分。
- 风险中(纯搬移,靠 89 测试兜底)。

### B3. 内置函数分发三处合一
- `typecheck.rs:702-888`(检查)、`codegen.rs:757-788`(type_hint)、
  `codegen.rs:1046-1216`(发射)手工同步。
- 方案:一张 `BuiltinSig` 表(名字、参数个数/类型、返回类型、
  是否复合),三处从表驱动。新增内置函数只改一处。

### B4. 其余重复
- 结构体字面量检查两份(`typecheck.rs:638-698` vs `:1170-1241`)合一;
- `lvalue_type`(`:511-559`)与 `check_expr` 的 Index/Field 臂合一;
- 字段 GEP 降低三份(`codegen.rs:889/965/1288`)→ `field_ptr(base, sname, fname)`;
- `load_u8`/`store_u8` 指针计算两份(`:1109` vs `:1165`);
- `adv!` 宏两份(`lexer.rs:91` vs `:480`)、字符串转义两份(`:290` vs `:592`)。

### B5. 清理
- 注释 mojibake(乱码破折号 `鈥?`):`lexer.rs:450`、`ast.rs:102/199`、
  `lib.rs:84/90`、`parser.rs:479`、`codegen.rs:421/436/728`、
  `typecheck.rs:1419` 等 —— 统一替换为 `—`;
- 死代码:`typecheck.rs:799` 恒空字符串条件;`:236/244` 重复 clear;
- `llvm.rs:5` blanket `allow(dead_code)` → 逐项标注,删掉未用 FFI;
- `parser.rs:232-244` `cur_len_params` 双重清空;`lexer.rs:450` 报错文案
  说 `'('` 但可能是 `[`。

---

## C. 健壮性补遗(P0 未尽项)

### C1. `ty_of` 返回 `Result`(消灭空类型 → LLVM 崩溃)
- `codegen.rs:144` 未知结构体回退 `null`,后续 LLVM 调用直接崩;
  `Type::Array` 长度超过 `u32::MAX`(`GENERIC_LEN` 泄漏、超长标注
  `[int; 50000000000]`)静默截断。
- 方案:`ty_of`/`const_int` 改 `Result<_, String>`,~25+8 个调用点加 `?`
  (两处闭包 `:250/:271` 改循环)。编译器-内部错误一律走 `internal` diag。

### C2. `--O0` 语义
- 现状:`--O0` 跳过 `LLVMRunPasses` 但 codegen level 仍是
  `CODEGEN_LEVEL_DEFAULT`(`codegen.rs:393`)→ 改为 `CODEGEN_LEVEL_NONE`。

### C3. clang 解析顺序
- `lib.rs:246-266` PATH 优先于仓库/安装目录的 clang —— PATH 上的旧
  clang 18/19 会遮蔽 LLVM 23 的 clang。考虑把 PATH 挪到最后,
  或在版本不匹配时告警。

### C4. 多错误收集(可选,产品向)
- 目前 fail-fast 只报第一个错误。`Diag` 收集成 `Vec`、前端各阶段尽量
  恢复并继续,`--json` 输出全部错误 —— 对 AI 消费体验提升明显,
  但涉及错误恢复策略,单独立项。

---

## D. 建议实施顺序

| 批次 | 内容 | 预计工作量 | 风险 |
|---|---|---|---|
| 1 | 度量(AOXN_TIME) + A1 + A2 + A5 | 半天 | 低 |
| 2 | A3 + A4 | 半天 | 中 |
| 3 | B1 + B5 | 半天 | 低 |
| 4 | B2 + B3 + B4 | 1 天 | 中 |
| 5 | C1 + C2 + C3 | 半天 | 低 |

每批之后跑全量 `cargo test`(89+)并对 `examples/bench_*.ax` 做一次
编译耗时回归对比。

## 明确不做(记录在案)

- 字符串表示改 fat pointer / length-prefix header(P1 已用"长度伴随
  slot + known_len 缓存"解决热点拼接;彻底改表示属 ABI 变更,
  待自举稳定后另议)。
- LLVM pass 定制、LTO、PGO —— 收益不明,等 C 组落地后再评估。
