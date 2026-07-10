import { SddError } from "../errors.js";
export class MissingTaskExecutor {
    async execute() {
        throw new SddError("E_COMPONENT_UNAVAILABLE", "宿主适配器必须为 sdd build 提供 TaskExecutor");
    }
}
//# sourceMappingURL=task-executor.js.map