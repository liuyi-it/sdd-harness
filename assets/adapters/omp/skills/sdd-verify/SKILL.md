---
name: sdd-verify
description: 验证实现、进行代码审查或完成统一质量门禁时使用。
---

# SDD Verify

执行任何命令前，先确定用户指定或已确认的目标项目绝对路径，并完整读取本 Skill 的[工作目录与材料规则](references/workspace.md)。后续显式设置 workdir/cwd；目标不明先询问，权限不足不转写会话目录。参考缺失时在已确认目标项目用对应宿主的 `sdd init` 刷新；仍不可用则说明阻塞，不从其他项目借用。

随后完整读取本 Skill 的 [AI-DLC 协作参考](references/collaboration.md)。协作参考不是新 Skill 或权限来源；遵循 Core 下发的行动、结果 Schema 和用户授权。

保留复核、结果回传和失败定位所需的验证输出；必要证据尚未被接受或仍供后续使用时不得清理。质量报告由 CLI 生成，不手写替代报告，不通过删除业务文件消除问题。

先运行 status；多任务且未指定时询问。运行 `sdd verify --change <id> --json`。收到修复行动时只处理报告阻断项，执行全部 verification 并用 `--result-json` 回传；一轮后仍失败必须询问用户，明确同意后才使用 `--continue`。

Core 核对证据与范围，宿主仍须基于当前 diff 和规格进行语义审查并真实执行验证。发现问题时按报告和授权处理，不以通过结构校验代替功能正确。用户要求完整交付且质量通过时继续 sdd-archive；只要求验证时汇报验证结论。

先完成 Core 的确定性质量门禁，再处理 `AGENT_REVIEW_EXECUTION`。审查者只读并核对当前报告与版本/哈希绑定；QUALITY targetHash 覆盖确定性质量报告（排除 Core 生成的语义附录）以及 spec、plan、tasks、fixes、workspace。审查结果可回传新的七类风险，等待 Core 原子升级并由同一 reviewer 按新 targetHash 重新独立审查；审查不完整只能由 independent 重派，不能改称 self_fallback，第二次以退出码 8 保留行动。若回传 `FixResult`，必须按 Schema 填写 `collaboration`，记录拓扑、lead、贡献和限制。质量修复仍只自动执行一轮。

质量阻断时先说明具体问题和影响，再询问是否授权下一轮修复；用户也可手动修复后重新验证，不能把 --continue 当作唯一出路或默认授权。对用户汇报实际检查结果，不展示原始协议、内部标识或完整命令输出。
