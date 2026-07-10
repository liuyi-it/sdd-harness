export async function runDesign(core, cwd, args, signal) {
    const request = {
        command: "design",
        cwd,
        args,
    };
    if (signal)
        request.signal = signal;
    return core.execute(request);
}
//# sourceMappingURL=design.js.map