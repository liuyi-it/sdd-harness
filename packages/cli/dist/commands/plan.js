export async function runPlan(core, cwd, args, signal) {
    const request = {
        command: "plan",
        cwd,
        args,
    };
    if (signal)
        request.signal = signal;
    return core.execute(request);
}
//# sourceMappingURL=plan.js.map