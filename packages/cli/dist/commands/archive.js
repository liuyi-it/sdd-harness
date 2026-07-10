export async function runArchive(core, cwd, args, signal) {
    const request = {
        command: "archive",
        cwd,
        args,
    };
    if (signal)
        request.signal = signal;
    return core.execute(request);
}
//# sourceMappingURL=archive.js.map