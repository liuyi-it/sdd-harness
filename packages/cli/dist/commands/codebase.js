/** sdd codebase 子命令分发 */
export async function runCodebase(core, cwd, subcommand, args, signal) {
    const validSubcommands = ["status", "doctor", "index", "query", "rebuild"];
    if (!subcommand || !validSubcommands.includes(subcommand)) {
        return {
            ok: false,
            state: "FAILED",
            exitCode: 2,
            error: {
                code: "E_INVALID_PHASE_COMMAND",
                message: `codebase 子命令必须为 ${validSubcommands.join(" / ")}，当前: ${subcommand ?? "(空)"}`,
                next: "sdd codebase status",
            },
        };
    }
    const request = {
        command: "codebase",
        cwd,
        args: { ...args, subcommand },
    };
    if (signal)
        request.signal = signal;
    return core.execute(request);
}
//# sourceMappingURL=codebase.js.map