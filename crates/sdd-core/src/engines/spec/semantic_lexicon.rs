//! 语义词典（翻译自 `packages/core/src/engines/spec/semantic-lexicon.ts`）。
//!
//! 用于动作检测与提取。JS 的 /i 标志在 Rust 中通过 RegexBuilder case_insensitive。

use regex::{Regex, RegexBuilder};

/// 动作检测器：行为是否包含可识别动作
pub fn action_detector() -> Regex {
    RegexBuilder::new(
        r"\b(cancel|cancellation|create|update|delete|query|search|get|read|return|respond)\b|取消|创建|更新|删除|查询|搜索|获取|读取|返回",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
}

/// 中文动作提取
pub fn action_extractor_zh() -> Regex {
    RegexBuilder::new(
        r"(创建用户|取消(?:待处理|未完成)?订单|(?:查询|搜索|获取|读取)[^，；,;]+|更新[^，；,;]+|删除[^，；,;]+)",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
}

/// 英文动作提取
pub fn action_extractor_en() -> Regex {
    RegexBuilder::new(
        r"\b(create\s+(?:a\s+)?user|cancel(?:lation|\s+(?:a\s+)?(?:pending\s+)?order)?|(?:query|search|get|read)\s+[^,;]+|update\s+[^,;]+|delete\s+[^,;]+)",
    )
    .case_insensitive(true)
    .build()
    .unwrap()
}

/// 文本是否含中文
pub fn is_chinese(value: &str) -> bool {
    value
        .chars()
        .any(|c| ('\u{3400}'..='\u{9fff}').contains(&c))
}
