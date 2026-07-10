interface AcquireOptions {
    timeoutMs?: number;
    retryDelayMs?: number;
}
export declare class FileLock {
    readonly root: string;
    private ownsLock;
    private readonly path;
    constructor(root: string);
    acquire(command: string, changeId?: string, options?: AcquireOptions): Promise<void>;
    private acquireUntil;
    release(): Promise<void>;
    private readExisting;
}
export {};
//# sourceMappingURL=file-lock.d.ts.map