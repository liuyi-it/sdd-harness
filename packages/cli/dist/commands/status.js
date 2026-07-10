export async function runStatus(core, cwd, args = {}, signal) {
    const request = {
        command: "status",
        cwd,
        args,
    };
    if (signal)
        request.signal = signal;
    return core.execute(request);
}
//# sourceMappingURL=status.js.map