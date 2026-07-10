export async function runReview(core, cwd, args, signal) {
    const request = {
        command: "review",
        cwd,
        args,
    };
    if (signal)
        request.signal = signal;
    return core.execute(request);
}
//# sourceMappingURL=review.js.map