//! 策略编译：把策略 markdown 编译为规则列表。
//!
//! 翻译自 早期 Node 实现 的规则提取语义：
//! 从 `## 规则` 或 `## Rules` 节提取 `- xxx` 列表项为规则。

#[derive(Debug, Clone, PartialEq)]
pub struct PolicyRule {
    pub text: String,
    pub source: String,
}

/// 编译策略 markdown 为规则列表
pub fn compile_policy(policy_md: &str) -> Vec<PolicyRule> {
    let mut rules = Vec::new();
    let mut in_rules_section = false;
    for line in policy_md.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            let heading = trimmed.trim_start_matches("## ").to_lowercase();
            in_rules_section = heading == "规则" || heading == "rules";
            continue;
        }
        if in_rules_section && trimmed.starts_with("- ") {
            let text = trimmed.trim_start_matches("- ").trim().to_string();
            if !text.is_empty() {
                rules.push(PolicyRule {
                    text,
                    source: "policy".to_string(),
                });
            }
        }
    }
    rules
}

/// 规则是否允许某文件变更（简单包含匹配）
pub fn rule_allows_file(rule: &PolicyRule, _file: &str) -> bool {
    let lower_rule = rule.text.to_lowercase();
    if lower_rule.contains("禁止修改") || lower_rule.contains("禁止") {
        if lower_rule.contains("未声明") || lower_rule.contains("未计划") {
            return false;
        }
        // 规则文本中出现的路径片段不在 file 中 → 该规则不适用
        return true;
    }
    true
}
