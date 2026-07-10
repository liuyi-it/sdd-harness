import fs from "node:fs/promises";
export async function runBuild(core, cwd, subcommand, taskId, resultPath, args, signal) {
    if (subcommand === "next") {
        const request = {
            command: "build",
            cwd,
            args: { ...args, subcommand: "next" },
        };
        if (signal)
            request.signal = signal;
        return core.execute(request);
    }
    if (subcommand === "complete") {
        if (!taskId || !resultPath) {
            return {
                ok: false,
                state: "FAILED",
                exitCode: 2,
                error: {
                    code: "E_INVALID_PHASE_COMMAND",
                    message: "build complete 需要 --task 和 --result 参数",
                },
            };
        }
        let resultJson;
        try {
            const raw = await fs.readFile(resultPath, "utf-8");
            resultJson = JSON.parse(raw);
        }
        catch {
            return {
                ok: false,
                state: "FAILED",
                exitCode: 4,
                error: {
                    code: "E_MISSING_ARTIFACT",
                    message: `无法读取或解析结果文件: ${resultPath}`,
                },
            };
        }
        const request = {
            command: "build",
            cwd,
            args: { ...args, subcommand: "complete", taskId, result: resultJson },
        };
        if (signal)
            request.signal = signal;
        return core.execute(request);
    }
    // 无子命令：返回 build 状态
    const request = {
        command: "build",
        cwd,
        args,
    };
    if (signal)
        request.signal = signal;
    return core.execute(request);
}
//# sourceMappingURL=build.js.map