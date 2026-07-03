# sdd-harness

`sdd-harness` 是一个面向 **Claude Code** 和 **Codex** 的 SDD 开发流程框架。

它用于把一次粗略的软件需求，转化为可执行、可验证、可追踪的开发流程，帮助 AI 编码工具按照更稳定的工程步骤完成开发任务。

---

## 项目目标

在日常使用 Claude Code 或 Codex 时，AI 很容易直接根据一句需求开始写代码，导致：

* 需求没有澄清清楚
* 修改范围不可控
* 缺少设计和任务拆解
* 测试和验收不完整
* 代码审查依赖人工兜底
* 变更过程不可追踪

`sdd-harness` 希望解决这些问题。

它会引导 AI 按照以下流程工作：

```text
初始化项目
  ↓
创建需求
  ↓
澄清需求
  ↓
生成规格文档
  ↓
生成设计方案
  ↓
拆解任务
  ↓
实现代码
  ↓
验证功能
  ↓
审查代码
  ↓
归档记录
```

---

## 适用场景

`sdd-harness` 适合用于：

* 使用 Claude Code 或 Codex 开发企业项目
* 希望 AI 编码过程更可控
* 希望每次需求变更都有文档记录
* 希望 AI 在写代码前先做需求澄清和方案设计
* 希望减少无关修改、过度设计和低质量实现
* 希望项目变更可以被验证、审查和归档

---

## 核心特性

### 1. 代码库感知

初始化项目时，`sdd-harness` 会建立当前项目的代码库上下文，让后续需求分析、方案设计和任务拆解基于真实代码结构进行。

### 2. 需求澄清

输入粗略需求后，`sdd-harness` 不会立即写代码，而是先进行需求分析，并自动提出需要确认的问题。

### 3. 阶段化开发

一次需求会被拆成多个清晰阶段：

```text
new → design → plan → build → verify → review → archive
```

每个阶段都有明确目标和输出。

### 4. 自动执行

可以通过一条命令自动完成完整流程：

```bash
sdd auto "实现订单取消功能"
```

如果中途遇到阻塞问题，流程会暂停，并提示下一步操作。

### 5. 可追踪制品

每次需求变更都会生成对应的文档和记录，便于后续回顾、审查和交接。

---

## 快速开始

### 1. 初始化项目

在项目根目录执行：

```bash
sdd init
```

初始化后，项目会生成 SDD 工作目录和 Claude Code / Codex 所需的配置。

---

### 2. 自动执行需求

```bash
sdd auto "实现订单取消功能"
```

该命令会自动完成：

```text
需求澄清
规格生成
设计方案
任务拆解
代码实现
功能验证
代码审查
归档记录
```

---

### 3. 查看当前状态

```bash
sdd status
```

示例输出：

```text
Project: order-service
Current Change: add-order-cancel
Current Phase: PLAN_READY
Index Status: READY

Next:
sdd build
```

---

## 手动流程

如果不希望全自动执行，也可以手动控制每个阶段。

```bash
sdd new "实现订单取消功能"
sdd design
sdd plan
sdd build
sdd verify
sdd review
sdd archive
```

---

## 命令说明

### `sdd init`

初始化当前项目，并建立代码库上下文。

```bash
sdd init
```

---

### `sdd auto`

自动执行完整 SDD 流程。

```bash
sdd auto "粗略需求"
```

示例：

```bash
sdd auto "实现订单取消功能"
```

---

### `sdd new`

创建新的需求变更，进行需求分析、澄清和规格生成。

```bash
sdd new "粗略需求"
```

---

### `sdd design`

基于需求规格生成设计方案。

```bash
sdd design
```

---

### `sdd plan`

基于设计方案拆解开发任务。

```bash
sdd plan
```

---

### `sdd build`

根据任务计划实现代码。

```bash
sdd build
```

---

### `sdd verify`

验证任务是否完成、功能边界是否满足。

```bash
sdd verify
```

---

### `sdd review`

审查代码质量、修改范围和实现合理性。

```bash
sdd review
```

---

### `sdd archive`

归档当前需求变更。

```bash
sdd archive
```

---

### `sdd status`

查看当前 SDD 状态和下一步建议。

```bash
sdd status
```

---

## Claude Code 使用方式

在 Claude Code 中，可以使用对应的 Slash Command：

```text
/sdd.init
/sdd.auto "实现订单取消功能"
/sdd.status
```

也可以手动执行阶段命令：

```text
/sdd.new "实现订单取消功能"
/sdd.design
/sdd.plan
/sdd.build
/sdd.verify
/sdd.review
/sdd.archive
```

---

## Codex 使用方式

在 Codex 中，可以直接使用：

```text
sdd init
sdd auto "实现订单取消功能"
sdd status
```

或者手动执行：

```text
sdd new "实现订单取消功能"
sdd design
sdd plan
sdd build
sdd verify
sdd review
sdd archive
```

---

## 生成内容

`sdd-harness` 会为每次需求变更生成对应记录，包括：

* 需求说明
* 澄清问题
* 需求规格
* 设计方案
* 任务拆解
* 测试计划
* 验证报告
* 审查报告
* 归档报告

这些内容会统一存放在项目的 `.sdd/` 目录中。

---

## 推荐使用方式

首次接入项目：

```bash
sdd init
```

日常开发需求：

```bash
sdd auto "你的需求描述"
```

复杂需求或高风险变更：

```bash
sdd new "你的需求描述"
sdd design
sdd plan
sdd build
sdd verify
sdd review
sdd archive
```

---

## 当前状态

项目当前处于设计和 MVP 实现阶段。

优先目标：

* 支持 Claude Code
* 支持 Codex
* 支持项目初始化
* 支持自动 SDD 流程
* 支持需求澄清、设计、任务拆解、实现、验证、审查和归档

---

## License

MIT
