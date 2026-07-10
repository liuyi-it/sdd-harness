import { createInterface } from "node:readline/promises";
import { getAvailableAdapters } from "@sdd-harness/core";
/**
 * CLI 层 init 命令：
 * 1. 如果指定了 --agent，校验是否都在可用列表中，不在则报错
 * 2. 如果未指定 --agent，进入交互式选择
 * 3. --non-interactive 且无 --agent 时报错
 * 4. 将选中的 agent 列表写入 args.agent 后调用 Core
 */
export async function runInit(core, cwd, args, signal) {
    const adapters = await getAvailableAdapters();
    if (args.agent !== undefined) {
        // --agent 指定模式：校验参数有效性
        const requestedAgents = typeof args.agent === "string"
            ? args.agent
                .split(",")
                .map((s) => s.trim())
                .filter((s) => s.length > 0)
            : Array.isArray(args.agent)
                ? args.agent.filter((a) => typeof a === "string")
                : [];
        const availableNames = new Set(adapters.map((a) => a.agent));
        const invalid = requestedAgents.filter((a) => !availableNames.has(a));
        if (invalid.length > 0) {
            return {
                ok: false,
                state: "NOT_INITIALIZED",
                exitCode: 2,
                error: {
                    code: "E_NOT_INITIALIZED",
                    message: `未知适配器: ${invalid.join(", ")}。可用: ${[...availableNames].join(", ")}`,
                },
            };
        }
        args = { ...args, agent: requestedAgents };
    }
    else {
        // 交互模式
        const nonInteractive = args["non-interactive"] === true || args.nonInteractive === true;
        if (nonInteractive) {
            return {
                ok: false,
                state: "NOT_INITIALIZED",
                exitCode: 2,
                error: {
                    code: "E_NOT_INITIALIZED",
                    message: "非交互模式下必须通过 --agent 指定要安装的适配器",
                },
            };
        }
        if (adapters.length === 0) {
            return {
                ok: false,
                state: "NOT_INITIALIZED",
                exitCode: 6,
                error: {
                    code: "E_COMPONENT_UNAVAILABLE",
                    message: "未检测到任何可用的 AI Agent 适配器",
                },
            };
        }
        console.log("检测到以下可用的 AI Agent 适配器，请选择要安装的（输入编号，多选用逗号分隔）：");
        adapters.forEach((adapter, index) => {
            console.log(`  ${index + 1}. ${adapter.agent}`);
        });
        console.log("");
        const rl = createInterface({
            input: process.stdin,
            output: process.stdout,
        });
        let selected = [];
        while (selected.length === 0) {
            const answer = await rl.question("请输入编号（至少选择一个）：");
            selected = parseSelection(answer, adapters.map((a) => a.agent));
            if (selected.length === 0) {
                console.log("输入无效或为空，请至少选择一个适配器。");
            }
        }
        rl.close();
        args = { ...args, agent: selected };
    }
    const request = {
        command: "init",
        cwd,
        args,
    };
    if (signal)
        request.signal = signal;
    return core.execute(request);
}
/**
 * 解析用户输入的编号字符串为 agent 名称数组。
 * 支持 "1"、"1,3"、"1, 3" 等格式。
 * 无效编号被忽略，返回空数组表示需要重新输入。
 */
function parseSelection(input, available) {
    const trimmed = input.trim();
    if (trimmed === "")
        return [];
    const indices = trimmed
        .split(/[,，]/)
        .map((s) => s.trim())
        .filter((s) => s.length > 0)
        .map((s) => {
        const n = Number(s);
        return Number.isInteger(n) && n >= 1 && n <= available.length
            ? n - 1
            : -1;
    })
        .filter((i) => i >= 0);
    // 去重
    const unique = [...new Set(indices)];
    return unique.map((i) => available[i]);
}
//# sourceMappingURL=init.js.map