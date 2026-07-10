interface TaskScope {
    allowedFiles: string[];
    expectedNewFiles: string[];
    forbiddenFiles: string[];
}
export declare function validateTaskFiles(files: string[], scope: TaskScope): void;
export {};
//# sourceMappingURL=task-scope.d.ts.map