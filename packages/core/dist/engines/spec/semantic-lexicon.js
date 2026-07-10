export const ACTION_DETECTOR = /\b(cancel|cancellation|create|update|delete|query|search|get|read|return|respond)\b|取消|创建|更新|删除|查询|搜索|获取|读取|返回/i;
export const ACTION_EXTRACTOR_ZH = /(创建用户|取消(?:待处理|未完成)?订单|(?:查询|搜索|获取|读取)[^，；,;]+|更新[^，；,;]+|删除[^，；,;]+)/i;
export const ACTION_EXTRACTOR_EN = /\b(create\s+(?:a\s+)?user|cancel(?:lation|\s+(?:a\s+)?(?:pending\s+)?order)?|(?:query|search|get|read)\s+[^,;]+|update\s+[^,;]+|delete\s+[^,;]+)/i;
//# sourceMappingURL=semantic-lexicon.js.map