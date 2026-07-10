export async function runVerify(core, cwd, args, signal) {
    const request = {
        command: "verify",
        cwd,
        args,
    };
    if (signal)
        request.signal = signal;
    return core.execute(request);
}
//# sourceMappingURL=verify.js.map