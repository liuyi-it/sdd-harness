//! 任务原子拆分器（翻译自 `packages/core/src/engines/superpowers/planner.ts`）。
//!
//! createAtomicTasks：从 spec/design/impact 生成 RED/GREEN/REFACTOR/VERIFY
//! 任务链，并对文件范围做精确映射与重叠检测。

use regex::Regex;
use regex::RegexBuilder;

use super::protocol::{
    done_criteria, phase_instruction, phase_title, PlanArtifacts, PlanningInput, RequirementPlan,
    TaskDefinition, PHASES,
};
use crate::engines::openspec::parser::parse_spec;
use crate::error::SddError;

// Rust regex 不支持 lookahead，去掉末尾 (?=$|...) 边界断言（捕获组提取语义不变）；
// 字符类内的 `[` 需转义（Rust 语法）
// toml 为 Rust 项目扩展（Node 版无此扩展名，Rust 版支持 Cargo 项目）
const FILE_PATTERN: &str = r#"(?:^|[\s`'"(\[])((?:(?:[A-Za-z]:)?\/{1,2})?(?:[\w@.-]+\/)*[\w@.-]+\.(?:properties|json|java|scala|swift|tsx|jsx|mjs|cjs|xml|ya?ml|kt|go|rs|py|rb|php|cs|ts|js|toml))"#;
const DIRECTORY_PATTERN: &str =
    r#"(?:^|[\s`'"(\[])((?:(?:[A-Za-z]:)?\/{1,2})?[\w@.-]+(?:\/[\w@.-]+)+\/)"#;

/// 生成原子任务链（语义对齐 createAtomicTasks）
pub fn create_atomic_tasks(
    input: &PlanningInput,
) -> Result<(Vec<TaskDefinition>, Vec<RequirementPlan>), SddError> {
    let requirements = parse_requirements(&input.spec)?;
    assert_requirements(&requirements)?;
    let context = format!("{}\n{}", input.impact, input.codebase_summary);
    let migration = requires_expand_contract(&input.design, &input.impact);
    let files = extract_paths(&context);
    let source_files: Vec<String> = files
        .iter()
        .filter(|f| is_source_file(f))
        .cloned()
        .collect();
    let test_files: Vec<String> = files.iter().filter(|f| is_test_file(f)).cloned().collect();
    if source_files.is_empty() || test_files.is_empty() {
        return Err(SddError::new(
            "E_UNRESOLVED_BLOCKER",
            "无法从 impact/codebaseSummary 推导精确的源码与测试文件范围，请先补充真实候选路径",
        )
        .with_next("sdd plan"));
    }

    let commands = detect_project_commands(&files);
    if commands.is_empty() {
        return Err(SddError::new(
            "E_UNRESOLVED_BLOCKER",
            "无法从 impact/codebaseSummary 识别项目验证命令，请补充 package.json 或 pom.xml 路径",
        )
        .with_next("sdd plan"));
    }

    let source_mapping = map_files(&requirements, &source_files, &context, is_source_file);
    let test_mapping = map_files(&requirements, &test_files, &context, is_test_file);
    let planned: Vec<RequirementPlan> = requirements
        .iter()
        .map(|r| RequirementPlan {
            id: r.id.clone(),
            title: r.title.clone(),
            scenarios: r.scenarios.clone(),
            source_files: source_mapping.get(&r.id).cloned().unwrap_or_default(),
            test_files: test_mapping.get(&r.id).cloned().unwrap_or_default(),
        })
        .collect();
    let new_files: std::collections::HashSet<String> =
        extract_new_paths(&context).into_iter().collect();

    let mut tasks: Vec<TaskDefinition> = Vec::new();
    let mut previous_chains: Vec<&RequirementPlan> = Vec::new();
    for (index, requirement) in planned.iter().enumerate() {
        let ordinal = format!("{:03}", index + 1);
        let scenario_ids: Vec<String> =
            requirement.scenarios.iter().map(|s| s.id.clone()).collect();
        let overlapping_verify_ids: Vec<String> = previous_chains
            .iter()
            .enumerate()
            .filter(|(_, previous)| overlaps(requirement, previous))
            .map(|(previous_index, _)| format!("TASK-{:03}-VERIFY", previous_index + 1))
            .collect();
        for (phase_index, phase) in PHASES.iter().enumerate() {
            let id = format!("TASK-{ordinal}-{phase}");
            let depends_on: Vec<String> = if migration && *phase == "VERIFY" {
                PHASES[..phase_index]
                    .iter()
                    .map(|p| format!("TASK-{ordinal}-{p}"))
                    .collect()
            } else if phase_index > 0 {
                vec![format!("TASK-{ordinal}-{}", PHASES[phase_index - 1])]
            } else {
                overlapping_verify_ids.clone()
            };
            let allowed_files = unique(
                requirement
                    .source_files
                    .iter()
                    .chain(requirement.test_files.iter())
                    .cloned()
                    .collect(),
            );
            let expected_new_files: Vec<String> = allowed_files
                .iter()
                .filter(|f| new_files.contains(*f))
                .cloned()
                .collect();
            tasks.push(TaskDefinition {
                id,
                title: format!("{}：{}", phase_title(phase), requirement.title),
                phase: phase.to_string(),
                status: "PENDING".to_string(),
                requirements: vec![requirement.id.clone()],
                scenarios: scenario_ids.clone(),
                depends_on,
                allowed_files,
                expected_new_files,
                forbidden_files: vec![".git/**".into(), ".env".into(), "**/credentials*".into()],
                verification: commands.clone(),
                done_criteria: done_criteria(phase, &scenario_ids),
                slice_type: Some(if migration {
                    match *phase {
                        "RED" => "EXPAND".to_string(),
                        "VERIFY" => "CONTRACT".to_string(),
                        _ => "MIGRATE".to_string(),
                    }
                } else {
                    "VERTICAL".to_string()
                }),
                user_visible_outcome: Some(format!(
                    "{} 的用户可见行为通过完整验证",
                    requirement.title
                )),
                acceptance_criteria: Some(done_criteria(phase, &scenario_ids)),
                test_seam: requirement.test_files.first().cloned(),
                policy_refs: None,
            });
        }
        previous_chains.push(requirement);
    }
    assert_acyclic(&tasks)?;
    Ok((tasks, planned))
}

/// 渲染任务 markdown（renderTasks）
pub fn render_tasks(tasks: &[TaskDefinition]) -> String {
    let mut lines = vec!["# Tasks".to_string()];
    for task in tasks {
        lines.push(String::new());
        lines.push(format!("## {}: {}", task.id, task.title));
        lines.push(String::new());
        lines.push(format!("Phase: {}", task.phase));
        lines.push(String::new());
        lines.push(format!("Status: {}", task.status));
        lines.push(String::new());
        lines.push(format!(
            "TDD Instruction: {}",
            phase_instruction(&task.phase)
        ));
        lines.push(String::new());
        push_list(&mut lines, "Requirements", &task.requirements);
        lines.push(String::new());
        push_list(&mut lines, "Scenarios", &task.scenarios);
        lines.push(String::new());
        push_list(&mut lines, "Depends On", &task.depends_on);
        lines.push(String::new());
        push_list(&mut lines, "Allowed Files", &task.allowed_files);
        lines.push(String::new());
        push_list(&mut lines, "Expected New Files", &task.expected_new_files);
        lines.push(String::new());
        push_list(&mut lines, "Forbidden Files", &task.forbidden_files);
        lines.push(String::new());
        push_list(&mut lines, "Verification", &task.verification);
        lines.push(String::new());
        push_list(&mut lines, "Done Criteria", &task.done_criteria);
    }
    lines.join("\n")
}

/// 渲染测试计划（renderTestPlan）
pub fn render_test_plan(requirements: &[RequirementPlan]) -> String {
    let mut lines = vec!["# Test Plan".to_string()];
    for requirement in requirements {
        for scenario in &requirement.scenarios {
            lines.push(String::new());
            lines.push(format!("## {}: {}", scenario.id, scenario.title));
            lines.push(String::new());
            lines.push(format!(
                "Requirement: {} {}",
                requirement.id, requirement.title
            ));
            lines.push(String::new());
            lines.push("- RED：先实现能因目标行为缺失而失败的场景测试。".to_string());
            lines.push("- 正向路径：验证 Scenario 定义的成功结果。".to_string());
            lines.push("- 反向路径：验证前置条件不满足、无效输入或边界失败。".to_string());
            lines.push("- VERIFY：执行项目完整验证命令并保留结果。".to_string());
        }
    }
    lines.join("\n")
}

/// 渲染单任务 Context Pack（renderContextPack）
pub fn render_context_pack(task: &TaskDefinition) -> String {
    let mut lines = vec![
        format!("# Context Pack: {}", task.id),
        String::new(),
        "## Task".to_string(),
        String::new(),
        task.title.clone(),
        String::new(),
        format!("Phase: {}", task.phase),
        String::new(),
        "## TDD Instruction".to_string(),
        String::new(),
        phase_instruction(&task.phase).to_string(),
        String::new(),
        "## Relevant Code Context".to_string(),
        String::new(),
        "按 Context Pack v2 References 中的 codebase 路径读取，不在此复制代码库摘要。".to_string(),
    ];
    push_list(&mut lines, "Requirements", &task.requirements);
    lines.push(String::new());
    push_list(&mut lines, "Scenarios", &task.scenarios);
    lines.push(String::new());
    push_list(&mut lines, "Expected New Files", &task.expected_new_files);
    lines.push(String::new());
    push_list(&mut lines, "Allowed Files", &task.allowed_files);
    lines.push(String::new());
    push_list(&mut lines, "Forbidden Files", &task.forbidden_files);
    lines.push(String::new());
    push_list(&mut lines, "Verification", &task.verification);
    lines.push(String::new());
    lines.push("## Risk".to_string());
    lines.push(String::new());
    lines.push("不得扩大文件范围或绕过现有安全与架构边界。".to_string());
    lines.join("\n")
}

/// 组装 PlanArtifacts（对齐 generatePlan）
pub fn build_plan_artifacts(input: &PlanningInput) -> Result<PlanArtifacts, SddError> {
    let (tasks, requirements) = create_atomic_tasks(input)?;
    let context = [
        "# Change Context".to_string(),
        String::new(),
        "## Codebase".to_string(),
        String::new(),
        input.codebase_summary.clone(),
        String::new(),
        "## Impact".to_string(),
        String::new(),
        input.impact.clone(),
        String::new(),
        "## Design".to_string(),
        String::new(),
        input.design.clone(),
    ]
    .join("\n");
    let context_packs: std::collections::HashMap<String, String> = tasks
        .iter()
        .map(|task| (task.id.clone(), render_context_pack(task)))
        .collect();
    let tasks_markdown = render_tasks(&tasks);
    let test_plan = render_test_plan(&requirements);
    Ok(PlanArtifacts {
        tasks,
        tasks_markdown,
        test_plan,
        context,
        context_packs,
    })
}

fn push_list(lines: &mut Vec<String>, title: &str, values: &[String]) {
    lines.push(format!("{title}:"));
    if values.is_empty() {
        lines.push("- None".to_string());
    } else {
        for value in values {
            lines.push(format!("- {value}"));
        }
    }
}

fn requires_expand_contract(design: &str, impact: &str) -> bool {
    let combined = format!("{design}\n{impact}");
    let re = RegexBuilder::new(
        r"(?:expand[–-]migrate[–-]contract|schema migration|database migration|兼容迁移|数据迁移|扩展.*迁移.*收缩)",
    )
    .case_insensitive(true)
    .build()
    .unwrap();
    re.is_match(&combined)
}

fn assert_acyclic(tasks: &[TaskDefinition]) -> Result<(), SddError> {
    let by_id: std::collections::HashMap<&str, &TaskDefinition> =
        tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut visiting: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();

    fn visit(
        id: &str,
        by_id: &std::collections::HashMap<&str, &TaskDefinition>,
        visiting: &mut std::collections::HashSet<String>,
        visited: &mut std::collections::HashSet<String>,
    ) -> Result<(), SddError> {
        if visiting.contains(id) {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                &format!("任务依赖图存在循环：{id}"),
            ));
        }
        if visited.contains(id) {
            return Ok(());
        }
        visiting.insert(id.to_string());
        if let Some(task) = by_id.get(id) {
            for dependency in &task.depends_on {
                visit(dependency, by_id, visiting, visited)?;
            }
        }
        visiting.remove(id);
        visited.insert(id.to_string());
        Ok(())
    }

    for task in tasks {
        visit(&task.id, &by_id, &mut visiting, &mut visited)?;
    }
    Ok(())
}

/// 从文本提取文件路径（extractPaths）
pub fn extract_paths(text: &str) -> Vec<String> {
    let file_re = Regex::new(FILE_PATTERN).unwrap();
    let dir_re = Regex::new(DIRECTORY_PATTERN).unwrap();
    let mut paths: Vec<String> = Vec::new();
    for raw_line in text.split('\n') {
        let line = raw_line.replace('\\', "/");
        for caps in file_re.captures_iter(&line) {
            let path = caps.get(1).unwrap().as_str().to_string();
            if is_safe_relative_path(&path) {
                paths.push(path);
            }
        }
        for caps in dir_re.captures_iter(&line) {
            let directory = caps.get(1).unwrap().as_str().to_string();
            if is_safe_focused_directory(&directory) {
                paths.push(format!("{directory}**"));
            }
        }
    }
    unique(paths)
}

fn extract_new_paths(text: &str) -> Vec<String> {
    let mut new_paths: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let normalized = line.replace('\\', "/");
        for path in extract_paths(&normalized) {
            let raw_path = path.strip_suffix("/**").unwrap_or(&path).to_string();
            let escaped = regex::escape(&raw_path);
            let re1 = RegexBuilder::new(&format!(r"新增\s+{escaped}"))
                .case_insensitive(true)
                .build()
                .unwrap();
            let re2 = RegexBuilder::new(&format!(r"{escaped}\s*(?:\(new\)|\[new\])"))
                .case_insensitive(true)
                .build()
                .unwrap();
            if re1.is_match(&normalized) || re2.is_match(&normalized) {
                new_paths.push(path);
            }
        }
    }
    unique(new_paths)
}

fn parse_requirements(spec: &str) -> Result<Vec<RequirementPlan>, SddError> {
    match parse_spec(spec) {
        Ok(document) if !document.requirements.is_empty() => Ok(document
            .requirements
            .iter()
            .map(|r| RequirementPlan {
                id: r.id.clone(),
                title: r.title.clone(),
                scenarios: r
                    .scenarios
                    .iter()
                    .map(|s| super::protocol::ScenarioRef {
                        id: s.id.clone(),
                        title: s.title.clone(),
                    })
                    .collect(),
                source_files: Vec::new(),
                test_files: Vec::new(),
            })
            .collect()),
        _ => {
            // 兼容旧 REQ 格式（### REQ-001: title）
            let heading_re = Regex::new(r"(?m)^### REQ-(\d+)(?::\s*(.*))?$").unwrap();
            let scenario_re = Regex::new(r"(?m)^#### Scenario:\s*(.+)$").unwrap();
            let headings: Vec<_> = heading_re.captures_iter(spec).collect();
            let mut plans = Vec::new();
            for (index, heading) in headings.iter().enumerate() {
                let id = format!("REQ-{}", heading.get(1).unwrap().as_str());
                let start = heading.get(0).unwrap().end();
                let end = headings
                    .get(index + 1)
                    .map(|h| h.get(0).unwrap().start())
                    .unwrap_or(spec.len());
                let body = &spec[start..end];
                let scenarios: Vec<super::protocol::ScenarioRef> = scenario_re
                    .captures_iter(body)
                    .enumerate()
                    .map(|(sc_index, caps)| super::protocol::ScenarioRef {
                        id: format!("{id}-SC-{:03}", sc_index + 1),
                        title: caps.get(1).unwrap().as_str().trim().to_string(),
                    })
                    .collect();
                let id_clone = id.clone();
                plans.push(RequirementPlan {
                    id,
                    title: heading
                        .get(2)
                        .map(|m| m.as_str().trim().to_string())
                        .filter(|s| !s.is_empty())
                        .unwrap_or(id_clone),
                    scenarios,
                    source_files: Vec::new(),
                    test_files: Vec::new(),
                });
            }
            Ok(plans)
        }
    }
}

/// 需求 → 文件映射（mapFiles）
fn map_files(
    requirements: &[RequirementPlan],
    files: &[String],
    context: &str,
    category: fn(&str) -> bool,
) -> std::collections::HashMap<String, Vec<String>> {
    let mut mapping: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for requirement in requirements {
        let explicit: Vec<String> = context
            .split('\n')
            .filter(|line| line_explicitly_references(line, requirement))
            .flat_map(extract_paths)
            .filter(|f| files.contains(f) && category(f))
            .collect();
        if !explicit.is_empty() {
            mapping.insert(requirement.id.clone(), unique(explicit));
        }
    }
    let unresolved: Vec<&RequirementPlan> = requirements
        .iter()
        .filter(|r| !mapping.contains_key(&r.id))
        .collect();
    if files.len() == 1 {
        for requirement in unresolved {
            mapping.insert(requirement.id.clone(), files.to_vec());
        }
        return mapping;
    }
    for file in files {
        let owners: Vec<&RequirementPlan> = unresolved
            .iter()
            .filter(|requirement| {
                requirement_tokens(requirement)
                    .iter()
                    .any(|token| file.to_lowercase().contains(token))
            })
            .copied()
            .collect();
        if owners.len() == 1 {
            let owner = owners[0];
            let entry = mapping.entry(owner.id.clone()).or_default();
            if !entry.contains(file) {
                entry.push(file.clone());
            }
        }
    }
    // 无映射需求：Rust 版返回空映射（调用方在 create_atomic_tasks 前已校验）
    mapping
}

fn line_explicitly_references(line: &str, requirement: &RequirementPlan) -> bool {
    let id_re = Regex::new(r"(?i)REQ-\d+").unwrap();
    let ids: Vec<String> = id_re
        .captures_iter(line)
        .map(|c| c.get(0).unwrap().as_str().to_uppercase())
        .collect();
    if !ids.is_empty() {
        return ids.contains(&requirement.id.to_uppercase());
    }
    let normalized = line.trim().trim_start_matches(['-', '*']).to_lowercase();
    let title = requirement.title.to_lowercase();
    normalized.starts_with(&format!("{title}:"))
        || normalized.starts_with(&format!("requirement: {title}"))
}

fn requirement_tokens(requirement: &RequirementPlan) -> Vec<String> {
    requirement
        .title
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && !c.is_alphabetic())
        .filter(|token| token.chars().count() >= 3)
        .map(String::from)
        .collect()
}

fn is_test_file(file: &str) -> bool {
    let re = Regex::new(r"(^|/)(?:test|tests|__tests__)(/|$)|\.(?:test|spec)\.[^.]+$").unwrap();
    re.is_match(file)
}

fn is_source_file(file: &str) -> bool {
    if is_test_file(file) {
        return false;
    }
    if Regex::new(r"(?i)^(?:package\.json|pom\.xml)$|/(?:package\.json|pom\.xml)$")
        .unwrap()
        .is_match(file)
    {
        return false;
    }
    file.ends_with("/**")
        || Regex::new(r"\.(?:ts|tsx|js|jsx|mjs|cjs|java|kt|go|rs|py|rb|php|cs|swift|scala)$")
            .unwrap()
            .is_match(file)
}

fn is_safe_focused_directory(directory: &str) -> bool {
    if !is_safe_relative_path(directory) {
        return false;
    }
    let segments: Vec<&str> = directory.split('/').filter(|s| !s.is_empty()).collect();
    segments.len() >= 2
        && segments
            .iter()
            .all(|s| *s != "." && *s != ".." && *s != "**")
}

fn is_safe_relative_path(path: &str) -> bool {
    !path.starts_with('/')
        && !Regex::new(r"^[A-Za-z]:/").unwrap().is_match(path)
        && !path.starts_with("//")
        && path
            .split('/')
            .all(|segment| segment != "." && segment != "..")
}

fn overlaps(left: &RequirementPlan, right: &RequirementPlan) -> bool {
    let left_patterns: Vec<&str> = left
        .source_files
        .iter()
        .chain(left.test_files.iter())
        .map(|s| s.as_str())
        .collect();
    let right_patterns: Vec<&str> = right
        .source_files
        .iter()
        .chain(right.test_files.iter())
        .map(|s| s.as_str())
        .collect();
    left_patterns.iter().any(|left| {
        right_patterns
            .iter()
            .any(|right| pattern_overlaps(left, right))
    })
}

fn pattern_overlaps(left: &str, right: &str) -> bool {
    let (left_path, left_dir) = describe(left);
    let (right_path, right_dir) = describe(right);
    if left_path == right_path {
        return true;
    }
    if left_dir && is_child(&right_path, &left_path) {
        return true;
    }
    if right_dir && is_child(&left_path, &right_path) {
        return true;
    }
    false
}

fn describe(pattern: &str) -> (String, bool) {
    let normalized = pattern.replace('\\', "/");
    let normalized = normalized.strip_prefix("./").unwrap_or(&normalized);
    let directory = normalized.ends_with("/**");
    let path = if directory {
        normalized
            .trim_end_matches("/**")
            .trim_end_matches('/')
            .to_string()
    } else {
        normalized.trim_end_matches('/').to_string()
    };
    (path, directory)
}

fn is_child(path: &str, directory: &str) -> bool {
    path.starts_with(&format!("{directory}/"))
}

fn assert_requirements(requirements: &[RequirementPlan]) -> Result<(), SddError> {
    if requirements.is_empty() {
        return Err(
            SddError::new("E_UNRESOLVED_BLOCKER", "规格至少需要一个 Requirement")
                .with_next("sdd plan"),
        );
    }
    let without_scenario = requirements.iter().find(|r| r.scenarios.is_empty());
    if let Some(requirement) = without_scenario {
        return Err(SddError::new(
            "E_UNRESOLVED_BLOCKER",
            &format!("{} 至少需要一个 Scenario", requirement.id),
        )
        .with_next("sdd plan"));
    }
    Ok(())
}

/// 项目验证命令检测（detectProjectCommands，Rust 版同时识别 Cargo）
pub fn detect_project_commands(files: &[String]) -> Vec<String> {
    let normalized: Vec<String> = files.iter().map(|f| f.replace('\\', "/")).collect();
    let mut candidates: Vec<String> = Vec::new();
    if normalized
        .iter()
        .any(|f| f == "pom.xml" || f.ends_with("/pom.xml"))
    {
        candidates.push("mvn test".to_string());
        candidates.push("mvn verify".to_string());
    }
    if normalized
        .iter()
        .any(|f| f == "package.json" || f.ends_with("/package.json"))
    {
        candidates.push("npm test".to_string());
    }
    if normalized
        .iter()
        .any(|f| f == "Cargo.toml" || f.ends_with("/Cargo.toml"))
    {
        candidates.push("cargo test".to_string());
    }
    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|c| seen.insert(c.clone()))
        .collect()
}

fn unique(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            result.push(value);
        }
    }
    result
}
