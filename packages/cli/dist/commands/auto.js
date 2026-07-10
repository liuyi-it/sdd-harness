export async function runAuto(core, cwd, requirement, args, signal) {
    const request = {
        command: "auto",
        cwd,
        args: { ...args, requirement },
    };
    if (signal)
        request.signal = signal;
    return core.execute(request);
}
//# sourceMappingURL=auto.js.map