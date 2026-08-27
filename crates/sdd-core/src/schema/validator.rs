//! 内嵌最小 JSON Schema 校验器。
//!
//! 只实现当前内嵌 schema 使用的关键字：type、required、properties、propertyNames、
//! additionalProperties、items、enum、const、pattern、minimum、minLength、minItems、
//! uniqueItems、oneOf、anyOf 与同文档 `$ref`。不接受隐式迁移，也不引入通用
//! JSON Schema 运行时依赖。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use regex::Regex;

use crate::error::SddError;

/// 已注册的 8 个 schema（内嵌编译期内容）
pub const SCHEMAS: [(&str, &str); 8] = [
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
    (
        "runtime",
        include_str!("../../../../schemas/runtime.schema.json"),
    ),
    (
        "config",
        include_str!("../../../../schemas/config.schema.json"),
    ),
    ("spec", include_str!("../../../../schemas/spec.schema.json")),
];

fn parsed_schemas() -> &'static [serde_json::Value; SCHEMAS.len()] {
    static PARSED: OnceLock<[serde_json::Value; SCHEMAS.len()]> = OnceLock::new();
    PARSED.get_or_init(|| {
        std::array::from_fn(|index| {
            serde_json::from_str(SCHEMAS[index].1)
                .expect("内嵌 schema 必须在构建和测试阶段保持合法 JSON")
        })
    })
}

/// 校验文档；失败返回 E_STATE_CORRUPTED（含首个问题描述）
pub fn validate_json(name: &str, doc: &serde_json::Value) -> Result<(), SddError> {
    let index = SCHEMAS
        .iter()
        .position(|(candidate, _)| *candidate == name)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", &format!("未知 schema：{name}")))?;
    let schema = &parsed_schemas()[index];
    let problems = check_against(schema, doc, "", schema);
    match problems.first() {
        Some(p) => Err(SddError::new(
            "E_STATE_CORRUPTED",
            &format!("{} 校验失败：{}", name, p),
        )),
        None => Ok(()),
    }
}

/// 解析同文档根内引用（如 "#/properties/foo"）；不支持外部文档引用。
fn resolve_ref<'a>(root: &'a serde_json::Value, reference: &str) -> Option<&'a serde_json::Value> {
    let pointer = reference.strip_prefix('#')?;
    if pointer.is_empty() {
        return Some(root);
    }
    root.pointer(pointer)
}

/// 编译 pattern 正则并按模式缓存（schema pattern 为常量，进程内只编译一次）。
fn compile_pattern(pattern: &str) -> Regex {
    static CACHE: OnceLock<Mutex<HashMap<String, Regex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard
        .entry(pattern.to_string())
        .or_insert_with(|| Regex::new(pattern).expect("schema pattern 必须是合法正则"))
        .clone()
}

/// 递归检查，返回问题列表。`root` 为文档根 schema，用于解析 $ref。
fn check_against(
    schema: &serde_json::Value,
    doc: &serde_json::Value,
    path: &str,
    root: &serde_json::Value,
) -> Vec<String> {
    let mut problems = Vec::new();

    // $ref：同文档根内引用，先解析再继续校验
    if let Some(reference) = schema.get("$ref").and_then(|v| v.as_str()) {
        if let Some(resolved) = resolve_ref(root, reference) {
            problems.extend(check_against(resolved, doc, path, root));
        } else {
            problems.push(format!("{path}：无法解析引用 {reference}"));
        }
    }

    // oneOf：恰好一个子 schema 通过
    if let Some(one_of) = schema.get("oneOf").and_then(|v| v.as_array()) {
        let passes = one_of
            .iter()
            .filter(|sub| check_against(sub, doc, path, root).is_empty())
            .count();
        if passes != 1 {
            problems.push(format!(
                "{path}：oneOf 要求恰好一个分支通过（实际 {passes} 个）"
            ));
        }
    }

    // anyOf：至少一个子 schema 通过
    if let Some(any_of) = schema.get("anyOf").and_then(|v| v.as_array()) {
        let passes = any_of
            .iter()
            .filter(|sub| check_against(sub, doc, path, root).is_empty())
            .count();
        if passes == 0 {
            problems.push(format!("{path}：anyOf 要求至少一个分支通过"));
        }
    }

    // required
    if doc.is_object() {
        if let Some(required) = schema.get("required").and_then(|v| v.as_array()) {
            for field in required {
                if let Some(field_name) = field.as_str() {
                    if doc.get(field_name).is_none() {
                        problems.push(format!("{path}{field_name}：缺少必填字段（required）"));
                    }
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

    // const
    if let Some(expected) = schema.get("const") {
        if doc != expected {
            problems.push(format!("{path}：不等于 const 约束值"));
        }
    }

    // minLength（按 Unicode 标量值计数）
    if let Some(minimum) = schema.get("minLength").and_then(|v| v.as_u64()) {
        if let Some(value) = doc.as_str() {
            if u64::try_from(value.chars().count()).is_ok_and(|length| length < minimum) {
                problems.push(format!("{path}：字符串短于 minLength {minimum}"));
            }
        }
    }

    // minItems
    if let Some(minimum) = schema.get("minItems").and_then(|v| v.as_u64()) {
        if let Some(items) = doc.as_array() {
            if u64::try_from(items.len()).is_ok_and(|length| length < minimum) {
                problems.push(format!("{path}：数组短于 minItems {minimum}"));
            }
        }
    }

    // pattern（仅字符串）
    if let Some(pattern) = schema.get("pattern").and_then(|v| v.as_str()) {
        if let Some(value) = doc.as_str() {
            if !compile_pattern(pattern).is_match(value) {
                problems.push(format!("{path}：不匹配 pattern {pattern}"));
            }
        }
    }

    // minimum（仅数值）
    if let Some(minimum) = schema.get("minimum").and_then(|v| v.as_f64()) {
        if let Some(number) = doc.as_f64() {
            if number < minimum {
                problems.push(format!("{path}：小于最小值 {minimum}"));
            }
        }
    }

    // uniqueItems（仅数组）
    if schema.get("uniqueItems").and_then(|v| v.as_bool()) == Some(true) {
        if let Some(arr) = doc.as_array() {
            let mut seen = std::collections::HashSet::new();
            if arr.iter().any(|item| !seen.insert(item)) {
                problems.push(format!("{path}：存在重复元素（uniqueItems）"));
            }
        }
    }

    // properties（递归，仅对象）
    if doc.is_object() {
        if let Some(props) = schema.get("properties").and_then(|v| v.as_object()) {
            for (key, sub_schema) in props {
                if let Some(sub_doc) = doc.get(key) {
                    problems.extend(check_against(
                        sub_schema,
                        sub_doc,
                        &format!("{path}{key}."),
                        root,
                    ));
                }
            }
        }
        if let (Some(name_schema), Some(object)) = (schema.get("propertyNames"), doc.as_object()) {
            for name in object.keys() {
                problems.extend(check_against(
                    name_schema,
                    &serde_json::Value::String(name.clone()),
                    &format!("{path}{name}."),
                    root,
                ));
            }
        }
    }
    if let (Some(items), Some(arr)) = (schema.get("items"), doc.as_array()) {
        for (i, item) in arr.iter().enumerate() {
            problems.extend(check_against(items, item, &format!("{path}[{i}]."), root));
        }
    }
    // additionalProperties: 对象形式 = 额外属性值枚举（tasks/artifacts 的 value 枚举）
    if let Some(additional) = schema
        .get("additionalProperties")
        .filter(|value| value.is_object())
    {
        if let Some(obj) = doc.as_object() {
            let properties = schema.get("properties").and_then(|v| v.as_object());
            for (key, value) in obj {
                if properties.is_some_and(|declared| declared.contains_key(key)) {
                    continue;
                }
                problems.extend(check_against(
                    additional,
                    value,
                    &format!("{path}{key}."),
                    root,
                ));
            }
        }
    }
    // additionalProperties: false = 拒绝未在 properties 中声明的键
    if schema.get("additionalProperties").and_then(|v| v.as_bool()) == Some(false) {
        if let Some(obj) = doc.as_object() {
            let properties = schema.get("properties").and_then(|v| v.as_object());
            for key in obj.keys() {
                if !properties.is_some_and(|declared| declared.contains_key(key)) {
                    problems.push(format!(
                        "{path}{key}：未在 properties 中声明（additionalProperties=false）"
                    ));
                }
            }
        }
    }

    problems
}

#[cfg(test)]
mod tests {
    use super::check_against;
    use serde_json::json;

    #[test]
    fn additional_properties_false_rejects_unknown_keys() {
        let schema = json!({
            "type": "object",
            "properties": { "a": { "type": "string" } },
            "additionalProperties": false
        });
        let bad = json!({ "a": "x", "b": "y" });
        assert!(!check_against(&schema, &bad, "", &schema).is_empty());
        let good = json!({ "a": "x" });
        assert!(check_against(&schema, &good, "", &schema).is_empty());
    }

    #[test]
    fn additional_property_schema_only_checks_undeclared_keys() {
        let schema = json!({
            "type": "object",
            "properties": { "name": { "type": "string" } },
            "additionalProperties": { "type": "integer" }
        });
        assert!(check_against(
            &schema,
            &json!({ "name": "demo", "retries": 1 }),
            "",
            &schema
        )
        .is_empty());
        assert!(!check_against(
            &schema,
            &json!({ "name": "demo", "retries": "one" }),
            "",
            &schema,
        )
        .is_empty());
    }

    #[test]
    fn scalar_and_collection_size_keywords_are_enforced() {
        let string_schema = json!({ "type": "string", "minLength": 2, "const": "ok" });
        assert!(check_against(&string_schema, &json!("ok"), "", &string_schema).is_empty());
        assert!(!check_against(&string_schema, &json!("o"), "", &string_schema).is_empty());

        let array_schema = json!({ "type": "array", "minItems": 1 });
        assert!(!check_against(&array_schema, &json!([]), "", &array_schema).is_empty());

        let nullable = json!({
            "type": ["object", "null"],
            "required": ["value"],
            "properties": { "value": { "type": "string" } }
        });
        assert!(check_against(&nullable, &json!(null), "", &nullable).is_empty());
    }

    #[test]
    fn property_names_are_validated() {
        let schema = json!({
            "type": "object",
            "propertyNames": { "pattern": "^TASK-[0-9]+$" }
        });
        assert!(check_against(&schema, &json!({ "TASK-1": "ok" }), "", &schema).is_empty());
        assert!(!check_against(&schema, &json!({ "task-1": "bad" }), "", &schema).is_empty());
    }

    #[test]
    fn additional_properties_false_without_properties_rejects_all_keys() {
        let schema = json!({ "type": "object", "additionalProperties": false });
        assert!(check_against(&schema, &json!({}), "", &schema).is_empty());
        assert!(!check_against(&schema, &json!({ "unexpected": true }), "", &schema).is_empty());
    }

    #[test]
    fn unique_items_rejects_duplicates() {
        let schema = json!({
            "type": "array",
            "items": { "type": "string" },
            "uniqueItems": true
        });
        assert!(!check_against(&schema, &json!(["a", "a"]), "", &schema).is_empty());
        assert!(check_against(&schema, &json!(["a", "b"]), "", &schema).is_empty());
    }

    #[test]
    fn one_of_requires_exactly_one_branch() {
        let schema = json!({ "oneOf": [{ "type": "string" }, { "type": "integer" }] });
        assert!(check_against(&schema, &json!("x"), "", &schema).is_empty());
        assert!(check_against(&schema, &json!(1), "", &schema).is_empty());
        assert!(!check_against(&schema, &json!(true), "", &schema).is_empty());
    }

    #[test]
    fn any_of_accepts_at_least_one_branch() {
        let schema = json!({ "anyOf": [{ "type": "string" }, { "type": "null" }] });
        assert!(check_against(&schema, &json!("x"), "", &schema).is_empty());
        assert!(check_against(&schema, &json!(null), "", &schema).is_empty());
        assert!(!check_against(&schema, &json!(42), "", &schema).is_empty());
    }

    #[test]
    fn ref_resolves_within_same_document() {
        let schema = json!({
            "$defs": { "name": { "type": "string", "pattern": "^[a-z]+$" } },
            "type": "object",
            "properties": { "name": { "$ref": "#/$defs/name" } }
        });
        assert!(check_against(&schema, &json!({ "name": "abc" }), "", &schema).is_empty());
        assert!(!check_against(&schema, &json!({ "name": "ABC123" }), "", &schema).is_empty());
        // 未解析的引用报告问题而非崩溃
        let dangling = json!({ "$ref": "#/$defs/missing" });
        assert!(!check_against(&dangling, &json!(null), "", &schema).is_empty());
    }
}
