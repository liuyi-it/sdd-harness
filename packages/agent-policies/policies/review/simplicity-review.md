# Simplicity Review

在正确性、安全、Spec 和测试审查完成后，再审查实现是否包含不必要复杂度。

重点检查：

- 重复实现已有 helper、type 或 module；
- 手写标准库或平台已经提供的功能；
- 为一个实现创建接口或 Factory；
- 没有第二个消费者的抽象层；
- 没有 Requirement 或 Design 依据的配置项；
- 未计划新增依赖；
- 仅做参数转发的 wrapper；
- 没有实际用途的兼容代码；
- 可以删除但不影响行为的 dead flexibility。

输出 finding 时必须说明位置、可删除或替换内容、替代方案、是否确定性和预计减少的代码规模。

不得建议删除安全边界、输入验证、数据完整性检查、迁移和兼容逻辑、TDD、verification 或 Requirement 明确要求的功能。
