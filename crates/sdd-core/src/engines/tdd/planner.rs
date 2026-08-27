//! 任务原子拆分器。
//!
//! 从 spec/design/impact 生成 RED/GREEN/REFACTOR/VERIFY 任务链，
//! 并对文件范围做精确映射与重叠检测。

use std::sync::LazyLock;

use regex::Regex;
use regex::RegexBuilder;

use super::protocol::{
    done_criteria, phase_instruction, phase_title, PlanArtifacts, PlanningInput, RequirementPlan,
    TaskDefinition, PHASES,
};
use crate::engines::spec::renderer::render_spec;
use crate::engines::spec::SpecDocument;
use crate::error::SddError;

// Rust regex 不支持 lookahead，去掉末尾 (?=$|...) 边界断言（捕获组提取语义不变）；
// 字符类内的 `[` 需转义（Rust 语法）
// 同时识别常见源码、测试和项目清单扩展名。
const FILE_PATTERN: &str = r#"(?:^|[\s`'"(\[])((?:(?:[A-Za-z]:)?\/{1,2})?(?:[\w@.-]+\/)*[\w@.-]+\.(?:properties|json|java|scala|swift|tsx|jsx|mjs|cjs|xml|ya?ml|kt|go|rs|py|rb|php|cs|ts|js|toml))"#;
const DIRECTORY_PATTERN: &str =
    r#"(?:^|[\s`'"(\[])((?:(?:[A-Za-z]:)?\/{1,2})?[\w@.-]+(?:\/[\w@.-]+)+\/)"#;

static MIGRATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new(
        r"(?:expand[–-]migrate[–-]contract|schema migration|database migration|兼容迁移|数据迁移|扩展.*迁移.*收缩)",
    )
    .case_insensitive(true)
    .build()
    .expect("迁移判定正则必须合法")
});
static FILE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(FILE_PATTERN).expect("文件路径正则必须合法"));
static DIRECTORY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(DIRECTORY_PATTERN).expect("目录路径正则必须合法"));
static REQUIREMENT_HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^### Requirement:.*$").expect("需求标题正则必须合法"));
static TEST_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(.*)\.(?:test|spec)(\.[^./]+)$").expect("测试文件名正则必须合法")
});
static REQUIREMENT_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)REQ-\d+").expect("需求 ID 正则必须合法"));
static TEST_FILE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|/)(?:test|tests|__tests__)(/|$)|\.(?:test|spec)\.[^.]+$")
        .expect("测试路径正则必须合法")
});
static PROJECT_MANIFEST_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:package\.json|pom\.xml)$|/(?:package\.json|pom\.xml)$")
        .expect("项目清单正则必须合法")
});
static SOURCE_FILE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\.(?:ts|tsx|js|jsx|mjs|cjs|java|kt|go|rs|py|rb|php|cs|swift|scala)$")
        .expect("源码扩展名正则必须合法")
});

/// 从当前规格、设计、影响和代码库摘要生成原子任务链。
pub fn create_atomic_tasks(
    input: &PlanningInput<'_>,
) -> Result<(Vec<TaskDefinition>, Vec<RequirementPlan>), SddError> {
    let specification = render_spec(input.specification).map_err(|error| {
        SddError::new("E_STATE_CORRUPTED", &format!("规格模型无法渲染：{error}"))
    })?;
    let requirements = requirement_plans(input.specification);
    assert_requirements(&requirements)?;
    let context = format!(
        "{}\n{}\n{}\n{}",
        specification, input.design, input.impact, input.codebase_summary
    );
    let migration = requires_expand_contract(input.design, input.impact);
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
            "无法从 spec/design/impact/codebaseSummary 推导精确的源码与测试文件范围，请在需求中明确候选路径",
        )
        .with_next("sdd plan"));
    }

    let commands = detect_project_commands(&files);
    if commands.is_empty() {
        return Err(SddError::new(
            "E_UNRESOLVED_BLOCKER",
            "无法从 spec/design/impact/codebaseSummary 识别项目验证命令，请补充 package.json 或 Cargo.toml 路径",
        )
        .with_next("sdd plan"));
    }

    let mut source_mapping = map_files(&requirements, &source_files, &context, is_source_file);
    merge_spec_path_mapping(
        &mut source_mapping,
        &specification,
        &source_files,
        is_source_file,
        &requirements,
    );
    let mut test_mapping = map_files(&requirements, &test_files, &context, is_test_file);
    merge_spec_path_mapping(
        &mut test_mapping,
        &specification,
        &test_files,
        is_test_file,
        &requirements,
    );
    let related_sources = related_source_files(&source_files, &test_files);
    if !related_sources.is_empty() {
        for requirement in &requirements {
            source_mapping
                .entry(requirement.id.clone())
                .or_insert_with(|| related_sources.clone());
        }
    }
    let focused_files = extract_paths(&format!(
        "{}\n{}\n{}",
        specification, input.design, input.impact
    ));
    map_single_requirement(
        &mut source_mapping,
        &mut test_mapping,
        &requirements,
        &focused_files,
    );

    if let Some(unmapped) = requirements.iter().find(|requirement| {
        !source_mapping
            .get(&requirement.id)
            .is_some_and(|files| !files.is_empty())
            || !test_mapping
                .get(&requirement.id)
                .is_some_and(|files| !files.is_empty())
    }) {
        return Err(SddError::new(
            "E_UNRESOLVED_BLOCKER",
            &format!(
                "{} 没有同时映射到源码和测试文件；请在 spec/design 中写出明确相对路径",
                unmapped.id
            ),
        )
        .with_next("sdd plan"));
    }

    let planned: Vec<RequirementPlan> = requirements
        .iter()
        .map(|r| RequirementPlan {
            id: r.id.clone(),
            title: r.title.clone(),
            scenarios: r.scenarios.clone(),
            source_files: source_mapping
                .get(&r.id)
                .expect("上方已验证源码映射")
                .clone(),
            test_files: test_mapping.get(&r.id).expect("上方已验证测试映射").clone(),
        })
        .collect();
    let new_files: std::collections::HashSet<String> =
        extract_new_paths(&context).into_iter().collect();

    let mut tasks: Vec<TaskDefinition> = Vec::new();
    for (index, requirement) in planned.iter().enumerate() {
        let ordinal = format!("{:03}", index + 1);
        let scenario_ids: Vec<String> =
            requirement.scenarios.iter().map(|s| s.id.clone()).collect();
        let acceptance_criteria: Vec<String> = requirement
            .scenarios
            .iter()
            .map(|scenario| format!("{}：{}", scenario.id, scenario.title))
            .collect();
        let overlapping_verify_ids: Vec<String> = planned[..index]
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
            let phase_files: Vec<String> = if *phase == "RED" {
                requirement.test_files.clone()
            } else {
                requirement
                    .source_files
                    .iter()
                    .chain(requirement.test_files.iter())
                    .cloned()
                    .collect()
            };
            let allowed_files = unique(phase_files);
            let expected_new_files: Vec<String> = allowed_files
                .iter()
                .filter(|file| new_files.contains(*file))
                .cloned()
                .collect();
            tasks.push(TaskDefinition {
                id,
                title: format!("{}：{}", phase_title(phase), requirement.title),
                phase: phase.to_string(),
                requirements: vec![requirement.id.clone()],
                scenarios: scenario_ids.clone(),
                depends_on,
                allowed_files,
                expected_new_files,
                forbidden_files: vec![
                    ".git/**".into(),
                    ".sdd/**".into(),
                    ".env".into(),
                    "**/credentials*".into(),
                ],
                verification: commands.clone(),
                done_criteria: done_criteria(phase, &scenario_ids),
                slice_type: if migration {
                    match *phase {
                        "RED" => "EXPAND".to_string(),
                        "VERIFY" => "CONTRACT".to_string(),
                        _ => "MIGRATE".to_string(),
                    }
                } else {
                    "VERTICAL".to_string()
                },
                user_visible_outcome: format!("{} 的用户可见行为通过完整验证", requirement.title),
                acceptance_criteria: acceptance_criteria.clone(),
                test_seam: requirement
                    .test_files
                    .first()
                    .cloned()
                    .expect("任务规划前已验证测试文件存在"),
            });
        }
    }
    validate_task_graph(&tasks)?;
    Ok((tasks, planned))
}

/// 渲染任务 Markdown。
pub fn render_tasks(tasks: &[TaskDefinition]) -> String {
    let mut lines = vec!["# 开发任务".to_string()];
    for task in tasks {
        lines.push(String::new());
        lines.push(format!("## [ ] {}：{}", task.id, task.title));
        lines.push(String::new());
        lines.push(format!("- 阶段：{}", task.phase));
        lines.push(String::new());
        lines.push(format!("- 执行要求：{}", phase_instruction(&task.phase)));
        lines.push(format!("- 切片类型：{}", task.slice_type));
        lines.push(format!("- 用户可见结果：{}", task.user_visible_outcome));
        lines.push(format!("- 测试 seam：{}", task.test_seam));
        lines.push(String::new());
        push_checklist(&mut lines, "关联需求", &task.requirements);
        lines.push(String::new());
        push_checklist(&mut lines, "关联场景", &task.scenarios);
        lines.push(String::new());
        push_checklist(&mut lines, "前置任务", &task.depends_on);
        lines.push(String::new());
        push_checklist(&mut lines, "允许修改", &task.allowed_files);
        lines.push(String::new());
        push_checklist(&mut lines, "预期新增", &task.expected_new_files);
        lines.push(String::new());
        push_checklist(&mut lines, "禁止修改", &task.forbidden_files);
        lines.push(String::new());
        push_checklist(&mut lines, "验证命令", &task.verification);
        lines.push(String::new());
        push_checklist(&mut lines, "完成标准", &task.done_criteria);
        lines.push(String::new());
        push_checklist(&mut lines, "验收标准", &task.acceptance_criteria);
    }
    lines.join("\n")
}

fn push_checklist(lines: &mut Vec<String>, title: &str, values: &[String]) {
    lines.push(format!("### {title}"));
    if values.is_empty() {
        lines.push("- [ ] 无".to_string());
    } else {
        for value in values {
            lines.push(format!("- [ ] {value}"));
        }
    }
}

/// 渲染测试计划。
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

/// 渲染单任务 Context Pack。
pub fn render_context_pack(task: &TaskDefinition) -> String {
    let mut lines = vec![
        format!("# Context Pack: {}", task.id),
        String::new(),
        "## Task".to_string(),
        String::new(),
        task.title.clone(),
        String::new(),
        format!("Phase: {}", task.phase),
        format!("Slice Type: {}", task.slice_type),
        String::new(),
        "## TDD Instruction".to_string(),
        String::new(),
        phase_instruction(&task.phase).to_string(),
        String::new(),
        "## User-visible Outcome".to_string(),
        String::new(),
        task.user_visible_outcome.clone(),
        String::new(),
        "## Test Seam".to_string(),
        String::new(),
        task.test_seam.clone(),
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
    push_list(&mut lines, "Acceptance Criteria", &task.acceptance_criteria);
    lines.push(String::new());
    lines.push("## Risk".to_string());
    lines.push(String::new());
    lines.push("不得扩大文件范围或绕过现有安全与架构边界。".to_string());
    lines.join("\n")
}

/// 组装计划任务、任务文档与测试计划。
pub fn build_plan_artifacts(input: &PlanningInput<'_>) -> Result<PlanArtifacts, SddError> {
    let (tasks, requirements) = create_atomic_tasks(input)?;
    let tasks_markdown = render_tasks(&tasks);
    let test_plan = render_test_plan(&requirements);
    Ok(PlanArtifacts {
        tasks,
        tasks_markdown,
        test_plan,
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
    MIGRATION_RE.is_match(&combined)
}

pub(crate) fn validate_task_graph(tasks: &[TaskDefinition]) -> Result<(), SddError> {
    let by_id: std::collections::HashMap<&str, &TaskDefinition> =
        tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    if by_id.len() != tasks.len() {
        return Err(SddError::new("E_STATE_CORRUPTED", "计划包含重复任务 ID"));
    }
    for task in tasks {
        if !super::protocol::valid_task_id(&task.id)
            || task.phase != task.id.rsplit('-').next().expect("有效任务 ID 包含阶段")
        {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                &format!("任务 {} 的 ID 与阶段不一致", task.id),
            ));
        }
        if let Some(missing) = task
            .depends_on
            .iter()
            .find(|dependency| !by_id.contains_key(dependency.as_str()))
        {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                &format!("任务 {} 依赖不存在的任务 {missing}", task.id),
            ));
        }
    }
    let mut visiting: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut visited: std::collections::HashSet<&str> = std::collections::HashSet::new();

    fn visit<'a>(
        id: &'a str,
        by_id: &std::collections::HashMap<&'a str, &'a TaskDefinition>,
        visiting: &mut std::collections::HashSet<&'a str>,
        visited: &mut std::collections::HashSet<&'a str>,
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
        visiting.insert(id);
        let task = by_id.get(id).expect("前置校验已确认所有依赖任务存在");
        for dependency in &task.depends_on {
            visit(dependency, by_id, visiting, visited)?;
        }
        visiting.remove(id);
        visited.insert(id);
        Ok(())
    }

    for task in tasks {
        visit(&task.id, &by_id, &mut visiting, &mut visited)?;
    }
    Ok(())
}

/// 从文本提取文件路径（extractPaths）
pub fn extract_paths(text: &str) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    for raw_line in text.split('\n') {
        let line = raw_line.replace('\\', "/");
        for caps in FILE_RE.captures_iter(&line) {
            let path = caps.get(1).unwrap().as_str().to_string();
            if is_safe_relative_path(&path) {
                paths.push(path);
            }
        }
        for caps in DIRECTORY_RE.captures_iter(&line) {
            let directory = caps.get(1).unwrap().as_str().to_string();
            if is_safe_focused_directory(&directory) {
                paths.push(format!("{directory}**"));
            }
        }
    }
    let normalized_text = text.replace('\\', "/");
    for auxiliary_path in [
        ".omp/commands/sdd.change.md",
        "assets/adapters/omp/commands/sdd.change.md",
        "README.md",
        "docs/**",
    ] {
        if normalized_text.contains(auxiliary_path) {
            paths.push(auxiliary_path.to_string());
        }
    }

    unique(paths)
}

fn extract_new_paths(text: &str) -> Vec<String> {
    let mut new_paths: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let paths = extract_paths(line);
        if paths.is_empty() {
            continue;
        }
        let normalized_lower = line.replace('\\', "/").to_lowercase();
        for path in paths {
            let raw_path = path.strip_suffix("/**").unwrap_or(&path);
            if is_marked_as_new(&normalized_lower, raw_path) {
                new_paths.push(path);
            }
        }
    }
    unique(new_paths)
}

/// `extract_new_paths` 仅在规划时调用，但每行可能包含多个路径；不在循环内动态编译正则。
/// 路径由当前提取器产生，非空，因此按命中位置推进不会出现空串循环。
fn is_marked_as_new(lower_line: &str, raw_path: &str) -> bool {
    let lower_path = raw_path.to_lowercase();
    has_added_prefix(lower_line, &lower_path) || has_new_suffix(lower_line, &lower_path)
}

fn has_added_prefix(line: &str, path: &str) -> bool {
    let mut remaining = line;
    while let Some(index) = remaining.find("新增") {
        let after_marker = &remaining[index + "新增".len()..];
        let after_space = after_marker.trim_start();
        if after_space.len() < after_marker.len() && after_space.starts_with(path) {
            return true;
        }
        remaining = after_marker;
    }
    false
}

fn has_new_suffix(line: &str, path: &str) -> bool {
    let mut remaining = line;
    while let Some(index) = remaining.find(path) {
        let after_path = &remaining[index + path.len()..];
        let marker = after_path.trim_start();
        if marker.starts_with("(new)") || marker.starts_with("[new]") {
            return true;
        }
        remaining = after_path;
    }
    false
}

fn requirement_plans(document: &SpecDocument) -> Vec<RequirementPlan> {
    document
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
        .collect()
}

/// 需求 → 文件映射（mapFiles）
fn map_files(
    requirements: &[RequirementPlan],
    files: &[String],
    context: &str,
    category: fn(&str) -> bool,
) -> std::collections::HashMap<String, Vec<String>> {
    let file_set: std::collections::HashSet<&str> = files.iter().map(String::as_str).collect();
    let mut mapping: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for requirement in requirements {
        let explicit: Vec<String> = context
            .split('\n')
            .filter(|line| line_explicitly_references(line, requirement))
            .flat_map(extract_paths)
            .filter(|file| file_set.contains(file.as_str()) && category(file))
            .collect();
        if !explicit.is_empty() {
            mapping.insert(requirement.id.clone(), unique(explicit));
        }
    }
    let unresolved: Vec<(&RequirementPlan, Vec<String>)> = requirements
        .iter()
        .filter(|r| !mapping.contains_key(&r.id))
        .map(|requirement| (requirement, requirement_tokens(requirement)))
        .collect();
    if files.len() == 1 {
        for (requirement, _) in unresolved {
            mapping.insert(requirement.id.clone(), files.to_vec());
        }
        return mapping;
    }
    for file in files {
        let lower = file.to_lowercase();
        let owners: Vec<&RequirementPlan> = unresolved
            .iter()
            .filter(|(_, tokens)| tokens.iter().any(|token| lower.contains(token)))
            .map(|(requirement, _)| *requirement)
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

fn merge_spec_path_mapping(
    mapping: &mut std::collections::HashMap<String, Vec<String>>,
    spec: &str,
    files: &[String],
    category: fn(&str) -> bool,
    requirements: &[RequirementPlan],
) {
    let file_set: std::collections::HashSet<&str> = files.iter().map(String::as_str).collect();
    let starts: Vec<usize> = REQUIREMENT_HEADING_RE
        .find_iter(spec)
        .map(|match_| match_.start())
        .collect();
    for (index, requirement) in requirements.iter().enumerate() {
        if mapping.contains_key(&requirement.id) {
            continue;
        }
        let Some(start) = starts.get(index).copied() else {
            continue;
        };
        let end = starts.get(index + 1).copied().unwrap_or(spec.len());
        let candidates = extract_paths(&spec[start..end])
            .into_iter()
            .filter(|path| file_set.contains(path.as_str()) && category(path))
            .collect::<Vec<_>>();
        if !candidates.is_empty() {
            mapping.insert(requirement.id.clone(), unique(candidates));
        }
    }
}

fn related_source_files(source_files: &[String], test_files: &[String]) -> Vec<String> {
    let source_set: std::collections::HashSet<&str> =
        source_files.iter().map(String::as_str).collect();
    unique(
        test_files
            .iter()
            .filter_map(|test| {
                let captures = TEST_NAME_RE.captures(test)?;
                let source = format!("{}{}", captures.get(1)?.as_str(), captures.get(2)?.as_str());
                source_set.contains(source.as_str()).then_some(source)
            })
            .collect(),
    )
}

fn map_single_requirement(
    source_mapping: &mut std::collections::HashMap<String, Vec<String>>,
    test_mapping: &mut std::collections::HashMap<String, Vec<String>>,
    requirements: &[RequirementPlan],
    focused_files: &[String],
) {
    if requirements.len() != 1 {
        return;
    }
    let requirement_id = requirements[0].id.clone();
    if !source_mapping
        .get(&requirement_id)
        .is_some_and(|files| !files.is_empty())
    {
        let files = focused_files
            .iter()
            .filter(|file| is_source_file(file))
            .cloned()
            .collect::<Vec<_>>();
        if !files.is_empty() {
            source_mapping.insert(requirement_id.clone(), files);
        }
    }
    if !test_mapping
        .get(&requirement_id)
        .is_some_and(|files| !files.is_empty())
    {
        let files = focused_files
            .iter()
            .filter(|file| is_test_file(file))
            .cloned()
            .collect::<Vec<_>>();
        if !files.is_empty() {
            test_mapping.insert(requirement_id, files);
        }
    }
}

fn line_explicitly_references(line: &str, requirement: &RequirementPlan) -> bool {
    if REQUIREMENT_ID_RE.is_match(line) {
        let expected_id = requirement.id.to_uppercase();
        return REQUIREMENT_ID_RE
            .find_iter(line)
            .any(|id| id.as_str().eq_ignore_ascii_case(&expected_id));
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
    TEST_FILE_RE.is_match(file)
}

fn is_source_file(file: &str) -> bool {
    if matches!(
        file,
        ".omp/commands/sdd.change.md" | "assets/adapters/omp/commands/sdd.change.md" | "README.md"
    ) {
        return true;
    }
    if is_test_file(file) {
        return false;
    }
    if PROJECT_MANIFEST_RE.is_match(file) {
        return false;
    }
    file.ends_with("/**") || SOURCE_FILE_RE.is_match(file)
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
        && !has_windows_drive_prefix(path)
        && !path.starts_with("//")
        && path
            .split('/')
            .all(|segment| segment != "." && segment != "..")
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && &bytes[1..3] == b":/"
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
    let mut candidates: Vec<String> = Vec::new();
    let has_manifest = |name| {
        files
            .iter()
            .any(|file| file.rsplit(['/', '\\']).next() == Some(name))
    };
    if has_manifest("pom.xml") {
        candidates.push("mvn test".to_string());
        candidates.push("mvn verify".to_string());
    }
    if has_manifest("package.json") {
        candidates.push("npm test".to_string());
    }
    if has_manifest("Cargo.toml") {
        candidates.push("cargo test".to_string());
    }
    candidates
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

#[cfg(test)]
mod tests {
    use super::{detect_project_commands, extract_new_paths, validate_task_graph, TaskDefinition};

    fn task(id: &str, phase: &str, depends_on: &[&str]) -> TaskDefinition {
        TaskDefinition {
            id: id.to_string(),
            title: "测试任务".to_string(),
            phase: phase.to_string(),
            requirements: vec!["REQ-001".to_string()],
            scenarios: vec!["SCN-001".to_string()],
            depends_on: depends_on
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            allowed_files: vec!["src/lib.rs".to_string()],
            expected_new_files: Vec::new(),
            forbidden_files: vec![".sdd/**".to_string()],
            verification: vec!["cargo test".to_string()],
            done_criteria: vec!["完成".to_string()],
            slice_type: "VERTICAL".to_string(),
            user_visible_outcome: "行为正确".to_string(),
            acceptance_criteria: vec!["SCN-001：行为正确".to_string()],
            test_seam: "src/lib.rs".to_string(),
        }
    }

    #[test]
    fn extracts_case_insensitive_new_path_markers() {
        let paths = extract_new_paths(
            "新增 crates/sdd-core/src/new_file.rs\ncrates/sdd-core/tests/new_file.test.rs [NEW]\ncrates/sdd-core/src/existing.rs",
        );

        assert!(paths
            .iter()
            .any(|path| path == "crates/sdd-core/src/new_file.rs"));
        assert!(paths
            .iter()
            .any(|path| path == "crates/sdd-core/tests/new_file.test.rs"));
        assert!(!paths
            .iter()
            .any(|path| path == "crates/sdd-core/src/existing.rs"));
    }

    #[test]
    fn detects_manifests_without_normalizing_every_path() {
        let commands = detect_project_commands(&[
            "backend\\pom.xml".to_string(),
            "frontend/package.json".to_string(),
            "crates/core/Cargo.toml".to_string(),
        ]);
        assert_eq!(
            commands,
            ["mvn test", "mvn verify", "npm test", "cargo test"]
        );
    }

    #[test]
    fn task_graph_rejects_missing_duplicate_cyclic_and_mismatched_nodes() {
        let red = task("TASK-001-RED", "RED", &[]);
        let green = task("TASK-001-GREEN", "GREEN", &["TASK-001-RED"]);
        assert!(validate_task_graph(&[red.clone(), green.clone()]).is_ok());

        assert!(validate_task_graph(&[
            red.clone(),
            task("TASK-001-GREEN", "GREEN", &["TASK-999-RED"]),
        ])
        .is_err());
        assert!(validate_task_graph(&[red.clone(), red]).is_err());
        assert!(
            validate_task_graph(&[task("TASK-001-RED", "RED", &["TASK-001-GREEN"]), green,])
                .is_err()
        );
        assert!(validate_task_graph(&[task("TASK-001-RED", "GREEN", &[])]).is_err());
    }
}
