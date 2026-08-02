//! 内嵌最小 JSON Schema 校验器。
//!
//! 校验语义对齐原 `scripts/validate-schemas.mjs` 的内联 validator：
//! 检查 required / enum / type（递归处理 properties 中的对象）。
//! 不引入外部 schema 校验 crate，保持依赖最小化。

use crate::error::SddError;

/// 已注册的 5 个 schema（内嵌编译期内容）
pub const SCHEMAS: [(&str, &str); 5] = [
    (
        "state",
        include_str!("../../../../schemas/state.schema.json"),
    ),
    ("task", include_str!("../../../../schemas/task.schema.json")),
    (
        "task-result",
        include_str!("../../../../schemas/task-result.schema.json"),
    ),
    (
        "report",
        include_str!("../../../../schemas/report.schema.json"),
    ),
    (
        "artifact",
        include_str!("../../../../schemas/artifact.schema.json"),
    ),
];

/// 校验文档；失败返回 E_STATE_CORRUPTED（含首个问题描述）
pub fn validate_json(name: &str, doc: &serde_json::Value) -> Result<(), SddError> {
    let (_, raw) = SCHEMAS
        .iter()
        .find(|(n, _)| *n == name)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", &format!("未知 schema：{name}")))?;
    let schema: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| SddError::new("E_STATE_CORRUPTED", &format!("schema 解析失败：{e}")))?;
    let problems = check_against(&schema, doc, "");
    match problems.first() {
        Some(p) => Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!("{} 校验失败：{}", name, p),
        )),
        None => Ok(()),
    }
}

/// 递归检查，返回问题列表
fn check_against(schema: &serde_json::Value, doc: &serde_json::Value, path: &str) -> Vec<String> {
    let mut problems = Vec::new();

    // required
    if let Some(required) = schema.get("required").and_then(|v| v.as_array()) {
        for field in required {
            if let Some(field_name) = field.as_str() {
                if !doc.get(field_name).is_some() {
                    problems.push(format!("{path}{field_name}：缺少必填字段（required）"));
                }
            }
        }
    }

    // type
    if let Some(t) = schema.get("type") {
        let ok = match t.as_str() {
            Some("object") => doc.is_object(),
            Some("array") => doc.is_array(),
            Some("string") => doc.is_string(),
            Some("integer") => doc.is_i64() || doc.is_u64(),
            Some("boolean") => doc.is_boolean(),
            Some("null") => doc.is_null(),
            _ => true,
        };
        // 联合类型 ["string", "null"] 等
        let ok = if let Some(types) = t.as_array() {
            types.iter().any(|t| match t.as_str() {
                Some("string") => doc.is_string(),
                Some("null") => doc.is_null(),
                Some("object") => doc.is_object(),
                Some("array") => doc.is_array(),
                Some("integer") => doc.is_i64() || doc.is_u64(),
                Some("boolean") => doc.is_boolean(),
                _ => true,
            })
        } else {
            ok
        };
        if !ok {
            problems.push(format!("{path}：类型不符，期望 {}", t));
        }
    }

    // enum
    if let Some(enum_values) = schema.get("enum").and_then(|v| v.as_array()) {
        if !enum_values.iter().any(|v| v == doc) {
            problems.push(format!("{path}：不在枚举范围内"));
        }
    }

    // properties（递归，仅对象）
    if doc.is_object() {
        if let Some(props) = schema.get("properties").and_then(|v| v.as_object()) {
            for (key, sub_schema) in props {
                if let Some(sub_doc) = doc.get(key) {
                    problems.extend(check_against(sub_schema, sub_doc, &format!("{path}{key}.")));
                }
            }
        }
    }
    if let (Some(items), Some(arr)) = (schema.get("items"), doc.as_array()) {
        for (i, item) in arr.iter().enumerate() {
            problems.extend(check_against(items, item, &format!("{path}[{i}].")));
        }
    }
    // additionalProperties: 字符串枚举映射（tasks/artifacts 的 value 枚举）
    if let Some(additional) = schema
        .get("additionalProperties")
        .and_then(|v| v.as_object())
    {
        if let Some(obj) = doc.as_object() {
            let additional_value = serde_json::Value::Object(additional.clone());
            for (_, value) in obj {
                problems.extend(check_against(&additional_value, value, path));
            }
        }
    }

    problems
}

/// 全部 schema 名称
pub fn schema_names() -> Vec<&'static str> {
    SCHEMAS.iter().map(|(n, _)| *n).collect()
}
