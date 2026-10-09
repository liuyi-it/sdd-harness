# Spring Boot 工单试用工程

这是 2026-10-09 Codex 宿主通过真实 `sdd` CLI 交付后的可复跑业务样例，不包含 Runtime、Agent 固定成功结果、构建产物或用户数据。它用于复现业务与验证命令；固定代码本身不能证明真实多 Agent 协作。

## 需求和边界

- `POST /api/tickets`：JSON `{customer_id: 正整数, title: 字符串}`；标题去首尾空白后为 1..120 个 Unicode 码点；返回 201 及 `ticket_id`、`customer_id`、`title`、`status=OPEN`。
- `GET /api/tickets/{ticket_id}`：已存在返回 200；任意未创建 ID（包括非 UUID 字符串）返回 404、`code=NOT_FOUND`。
- 无效类型、空标题、过长标题、未知字段和不合法 JSON 返回 400、`code=BAD_REQUEST`。
- 保留健康接口。内存存储支持并发创建，不覆盖已有工单，重启后清空；不包含数据库、认证、外部通知或前端。

## 复跑业务验收

需要 Java 17、Maven 和 Python 3；在独立目录复制本工程后执行。显式选择已有 Java 17，避免默认 Java 8 导致编译失败。Spring Boot 固定为本轮实际验证的 3.5.11，不表示最新版本。

```bash
cd <复制后的工程绝对路径>
export JAVA_HOME=<已有的Java17目录>
export PATH="$JAVA_HOME/bin:$PATH"
mvn -B verify
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s src/test/python -v
```

使用 Maven 私服时先确认可访问；需要隔离配置时将 `-s <settings.xml>` 作为独立参数，并把 localRepository 指向自己的临时缓存，不修改全局配置。冷缓存首次运行需要下载公开依赖，失败应区分依赖获取、Java 版本和业务测试错误。

本轮 Maven 实际通过 23 项测试：健康、创建/查询、严格字段类型、Unicode 长度、稳定错误和 20 并发创建后逐一读取。Python 的 1 项测试实际启动打包 JAR、发 HTTP 请求、终止并重新启动进程，证明旧工单不存在。进程仅监听本机随机端口，测试退出时清理自身进程。该进程检查在 macOS 验证，Windows 未验证。

## 重跑 SDD 宿主旅程

另建隔离工程作为未实现基线，复制 `.gitignore`、POM、application.properties、TicketApplication、HealthController 和 HealthTest；先验证健康检查并保存该模拟工程的 Git 基线。保留构建和 Python 缓存忽略规则，避免测试生成物使质量审查指纹失效。然后让宿主使用当前待验证的 `sdd`，从以下自然语言需求连续运行五阶段：

> 在现有 Spring Boot 工单服务中实现工单创建和查询，customer_id 为正整数，title 去空白后非空且最多 120 字。JSON 使用 snake_case，创建返回 201 和 OPEN 状态，输入错误或不合法 JSON 返回稳定 400，未知工单返回稳定 404。内存存储即可，重启清空；并发创建不覆盖。保留健康检查，不增加数据库、认证、通知或前端。添加真实 HTTP 测试，使用 Maven 验证并完成交付。

由宿主根据实际代码生成统一规格和纵向计划，独立审查 HTTP 合同与并发风险，派发单一开发子 Agent 先写测试取得真实 RED，再实现并完成 Maven 与 JAR 进程验证。规格、计划和归档始终由 CLI 生成；不把本目录代码、固定 JSON 或预填成功结论当作宿主试用证据。

检查中断后原命令恢复同一行动、修正结果后原命令回传，以及质量通过后修改代码会使归档被拒绝；重新执行实际验证和独立审查后再归档。完整问题、修复及证据边界见 [可用性试用记录](../../docs/usability.md)。
