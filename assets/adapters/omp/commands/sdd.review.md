---
description: 审查改动范围、安全和最小实现
---

使用 sdd-harness skill 执行 `sdd review --json`。若返回的 warnings 含 `W_OCR_NOT_FOUND`，表示可选 OCR 后端不可用，请按原版确定性 review 结论处理。将审查结论、阻断问题、风险和建议转换为简洁中文；不要展示 JSON、OCR prompt 或内部路径。

$ARGUMENTS
