export async function runNew(core, cwd, requirement, args, signal) {
    const request = {
        command: "new",
        cwd,
        args: { ...args, requirement },
    };
    if (signal)
        request.signal = signal;
    return core.execute(request);
}
//# sourceMappingURL=new.js.map