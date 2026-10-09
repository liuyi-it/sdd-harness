//! 阶段内的语义审查：Core 管理目标绑定与发现，宿主执行实际审查。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::contracts::{AgentActionRequired, CliWarning, CommandResult};
use crate::error::SddError;
use crate::git::GitInspector;
use crate::state::state_store::{apply_workflow_update, ChangeWorkflow};
use crate::state::{RuntimeDocument, RuntimeStore};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewPolicy {
    pub risk_factors: Vec<String>,
    pub rationale: String,
}

impl ReviewPolicy {
    pub fn independent_required(&self) -> bool {
        !self.risk_factors.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Recheck {
    finding_id: String,
    present: bool,
    explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubmittedFinding {
    severity: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewResult {
    review_id: String,
    target_hash: String,
    mode: String,
    verdict: String,
    summary: String,
    findings: Vec<SubmittedFinding>,
    rechecks: Vec<Recheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback_reason: Option<String>,
    #[serde(default)]
    incomplete: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    risk_factors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Finding {
    id: String,
    severity: String,
    message: String,
    file: Option<String>,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewRecord {
    id: String,
    target: String,
    target_hash: String,
    reviewer: String,
    independent_required: bool,
    incomplete_attempts: u8,
    superseded: bool,
    result: Option<ReviewResult>,
    findings: Vec<Finding>,
}

pub(crate) enum Completion {
    Ready,
    Rejected,
    Incomplete,
    Escalated,
}

fn records(change: &Value) -> Result<Vec<ReviewRecord>, SddError> {
    change.get("reviews").map_or_else(
        || Ok(Vec::new()),
        |value| {
            serde_json::from_value(value.clone()).map_err(|error| {
                SddError::new("E_STATE_CORRUPTED", &format!("审查记录无效：{error}"))
            })
        },
    )
}

pub(crate) fn validate_storage(change: &Value) -> Result<(), SddError> {
    if let Some(candidate) = change.get("candidate") {
        crate::engines::documents::parse_spec(candidate)?;
    }
    if let Some(risks) = change.get("reviewRiskFactors") {
        let wrapper = json!({"reviewId":"risk-validation","targetHash":"0".repeat(64),"mode":"self","verdict":"READY","summary":"风险校验","findings":[],"rechecks":[],"riskFactors":risks});
        crate::schema::validate_json("review-result", &wrapper)?;
    }
    let mut ids = BTreeSet::new();
    for record in records(change)? {
        if !ids.insert(record.id.clone())
            || !matches!(record.target.as_str(), "SPECIFICATION" | "QUALITY")
            || !matches!(
                record.reviewer.as_str(),
                "product-reviewer" | "architecture-reviewer"
            )
            || !valid_digest(&record.target_hash)
            || record.incomplete_attempts > 2
            || record.findings.iter().any(|finding| {
                !matches!(finding.status.as_str(), "OPEN" | "RESOLVED" | "SUPERSEDED")
                    || !matches!(
                        finding.severity.as_str(),
                        "critical" | "high" | "medium" | "low"
                    )
            })
        {
            return Err(SddError::new("E_STATE_CORRUPTED", "审查记录不变量无效"));
        }
        if let Some(result) = record.result {
            crate::schema::validate_json(
                "review-result",
                &serde_json::to_value(&result).expect("审查结果可序列化"),
            )?;
            if result.review_id != record.id || result.target_hash != record.target_hash {
                return Err(SddError::new("E_STATE_CORRUPTED", "审查结果与记录不匹配"));
            }
        }
    }
    Ok(())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn validate_pending(workflow: &ChangeWorkflow) -> Result<(), SddError> {
    let pending = workflow
        .pending_agent_action
        .as_ref()
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "审查等待缺少行动"))?;
    let expected = if workflow.phase == "SPEC_WAITING_REVIEW" {
        "SPECIFICATION"
    } else {
        "QUALITY"
    };
    if pending["type"] != "AGENT_REVIEW_EXECUTION"
        || pending["target"] != expected
        || !pending["reviewId"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
        || !pending["targetHash"].as_str().is_some_and(valid_digest)
    {
        return Err(SddError::new("E_STATE_CORRUPTED", "审查等待行动无效"));
    }
    Ok(())
}

pub(crate) fn policy(runtime: &RuntimeDocument, change_id: &str) -> Result<ReviewPolicy, SddError> {
    let change = &runtime.changes[change_id];
    let spec = change
        .get("candidate")
        .or_else(|| change.get("spec"))
        .ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "缺少审查策略"))?;
    let mut policy: ReviewPolicy = serde_json::from_value(spec["reviewPolicy"].clone())
        .map_err(|error| SddError::new("E_STATE_CORRUPTED", &error.to_string()))?;
    if let Some(risks) = change.get("reviewRiskFactors").and_then(Value::as_array) {
        for risk in risks {
            let risk = risk.as_str().expect("风险已校验");
            if !policy.risk_factors.iter().any(|item| item == risk) {
                policy.risk_factors.push(risk.into());
            }
        }
    }
    Ok(policy)
}

pub(crate) fn target_hash(
    cwd: &str,
    runtime: &RuntimeDocument,
    change_id: &str,
    target: &str,
) -> Result<String, SddError> {
    let workflow = super::workflow(runtime, change_id)?;
    let change = &runtime.changes[change_id];
    let run = &runtime.runs[&workflow.run_id];
    let value = if target == "SPECIFICATION" {
        json!({"input": run["input"], "candidate": change.get("candidate").ok_or_else(|| SddError::new("E_MISSING_ARTIFACT", "缺少待审查规格"))?, "risks":change.get("reviewRiskFactors")})
    } else {
        json!({"spec": change["spec"], "plan": change["plan"], "tasks": run["tasks"], "fixes": run.get("fixes"), "workspace": workspace_hash(cwd, runtime, change_id)?, "assessment":assessment_binding(change), "risks":change.get("reviewRiskFactors")})
    };
    let bytes = serde_json::to_vec(&value).expect("目标快照可序列化");
    Ok(crate::policies::digest::digest_bytes(&bytes))
}

pub(crate) fn workspace_hash(
    cwd: &str,
    runtime: &RuntimeDocument,
    change_id: &str,
) -> Result<String, SddError> {
    let workflow = super::workflow(runtime, change_id)?;
    let root = workflow
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.worktree_path.as_deref())
        .unwrap_or(cwd);
    if GitInspector::is_git_repo(root)? {
        return GitInspector::workspace_fingerprint(root);
    }
    // 无 Git 时只能绑定计划允许的文件范围；新增和删除均改变路径与内容快照。
    let tasks = super::plan::plan_tasks(&runtime.changes[change_id]["plan"])?;
    let allowed = tasks
        .iter()
        .flat_map(|task| task.allowed_files.iter().cloned())
        .collect::<Vec<_>>();
    let mut files = Vec::new();
    let mut prefixes = BTreeSet::new();
    for pattern in &allowed {
        let parts = pattern
            .split('/')
            .take_while(|part| !part.contains(['*', '?', '[', '{']))
            .collect::<Vec<_>>();
        let mut prefix = Path::new(root).to_path_buf();
        if parts.is_empty() {
            prefixes.insert(prefix);
            continue;
        }
        let mut leaf_type = None;
        for (index, part) in parts.iter().enumerate() {
            prefix.push(part);
            let metadata = match std::fs::symlink_metadata(&prefix) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    leaf_type = None;
                    break;
                }
                Err(error) => return Err(SddError::new("E_QUALITY_FAILED", &error.to_string())),
            };
            // 目录范围不支持链接别名；必须在遍历前拒绝，空目录也不能绕过。
            // 精确文件的末级链接按链接文本绑定，不读取或遍历其目标。
            if metadata.file_type().is_symlink()
                && (index + 1 < parts.len() || parts.len() < pattern.split('/').count())
            {
                return Err(SddError::new(
                    "E_PATH_OUTSIDE_REPO",
                    "无 Git 审查范围的目录前缀不得包含符号链接",
                ));
            }
            leaf_type = Some(metadata.file_type());
        }
        if leaf_type.is_some_and(|kind| kind.is_dir()) {
            prefixes.insert(prefix);
        } else if leaf_type.is_some() {
            files.push(
                prefix
                    .strip_prefix(root)
                    .expect("计划路径在项目内")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    let roots = prefixes
        .iter()
        .filter(|prefix| {
            !prefixes
                .iter()
                .any(|other| other != *prefix && prefix.starts_with(other))
        })
        .collect::<Vec<_>>();
    for prefix in roots {
        collect_files(Path::new(root), prefix, &mut files)?;
    }
    let files = crate::security::task_scope::matching_paths(&files, &allowed)?;
    let hashes = GitInspector::file_hashes(root, &files)?;
    Ok(crate::policies::digest::digest_bytes(
        &serde_json::to_vec(&hashes).expect("文件快照可序列化"),
    ))
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<String>) -> Result<(), SddError> {
    for entry in std::fs::read_dir(directory)
        .map_err(|error| SddError::new("E_QUALITY_FAILED", &error.to_string()))?
    {
        let entry = entry.map_err(|error| SddError::new("E_QUALITY_FAILED", &error.to_string()))?;
        let kind = entry
            .file_type()
            .map_err(|error| SddError::new("E_QUALITY_FAILED", &error.to_string()))?;
        let path = entry.path();
        if matches!(entry.file_name().to_str(), Some(".sdd" | ".git")) {
            continue;
        }
        if kind.is_dir() {
            collect_files(root, &path, files)?;
        } else {
            files.push(
                path.strip_prefix(root)
                    .expect("路径由根目录遍历生成")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

pub(crate) fn begin(
    cwd: &str,
    runtime: &RuntimeDocument,
    change_id: &str,
    target: &str,
    reviewer: &str,
) -> Result<CommandResult, SddError> {
    let independent_required = policy(runtime, change_id)?.independent_required();
    let hash = target_hash(cwd, runtime, change_id, target)?;
    let mut reviews = records(&runtime.changes[change_id])?;
    let id = format!(
        "REVIEW-{}-{:03}",
        super::workflow(runtime, change_id)?.run_id,
        reviews.len() + 1
    );
    reviews.push(ReviewRecord {
        id: id.clone(),
        target: target.to_string(),
        target_hash: hash.clone(),
        reviewer: reviewer.to_string(),
        independent_required,
        incomplete_attempts: 0,
        superseded: false,
        result: None,
        findings: Vec::new(),
    });
    RuntimeStore::new(cwd.to_string()).try_update(|document| {
        super::change_mut(document, change_id)?.insert("reviews".into(), serde_json::to_value(&reviews).expect("审查记录可序列化"));
        apply_workflow_update(super::workflow_mut(document, change_id)?, |workflow| {
            workflow.phase = if target == "SPECIFICATION" { "SPEC_WAITING_REVIEW" } else { "QUALITY_WAITING_REVIEW" }.to_string();
            workflow.pending_agent_action = Some(json!({"type":"AGENT_REVIEW_EXECUTION", "reviewId":id,"target":target,"targetHash":hash}));
            workflow.in_progress_phase = Some(target.to_string());
            workflow.suggested_command = Some(format!("sdd {} --change {change_id}", source_command(workflow, target)));
        })
    })?;
    action(cwd, &RuntimeStore::new(cwd.to_string()).read()?, change_id)
}

fn source_command<'a>(workflow: &'a ChangeWorkflow, target: &str) -> &'a str {
    if target == "QUALITY" {
        "verify"
    } else if workflow.last_command.as_deref() == Some("sdd change") {
        "change"
    } else {
        "spec"
    }
}

fn pending_record(runtime: &RuntimeDocument, change_id: &str) -> Result<ReviewRecord, SddError> {
    let workflow = super::workflow(runtime, change_id)?;
    validate_pending(workflow)?;
    let id = workflow.pending_agent_action.as_ref().expect("已校验")["reviewId"]
        .as_str()
        .expect("已校验");
    let record = records(&runtime.changes[change_id])?
        .into_iter()
        .find(|record| record.id == id && !record.superseded)
        .ok_or_else(|| SddError::new("E_STATE_CORRUPTED", "待处理审查不存在"))?;
    let pending = workflow.pending_agent_action.as_ref().expect("已校验");
    if pending["target"] != record.target || pending["targetHash"] != record.target_hash {
        return Err(SddError::new(
            "E_STATE_CORRUPTED",
            "待处理审查与目标记录不一致",
        ));
    }
    Ok(record)
}

pub(crate) fn action(
    cwd: &str,
    runtime: &RuntimeDocument,
    change_id: &str,
) -> Result<CommandResult, SddError> {
    let record = pending_record(runtime, change_id)?;
    if record.target_hash != target_hash(cwd, runtime, change_id, &record.target)? {
        return Err(SddError::new(
            "E_QUALITY_REQUIRED",
            "审查期间成果发生变化，请重新取得当前审查行动",
        ));
    }
    let change = &runtime.changes[change_id];
    let workflow = super::workflow(runtime, change_id)?;
    let snapshot = if record.target == "SPECIFICATION" {
        json!({"input":runtime.runs[&workflow.run_id]["input"],"candidate":change["candidate"]})
    } else {
        json!({"spec":change["spec"],"plan":change["plan"],"verification":runtime.runs[&workflow.run_id],"qualityReport":change.pointer("/reports/quality")})
    };
    let open = open_findings(&records(change)?, &record.target, &record.reviewer);
    let role = crate::assets::role_reference(&record.reviewer).expect("审查角色必须内嵌");
    let material = serde_json::to_string_pretty(&snapshot)
        .expect("快照可序列化")
        .replace(
            "END_UNTRUSTED_REVIEW_TARGET",
            "ESCAPED_END_UNTRUSTED_REVIEW_TARGET",
        );
    let command = source_command(workflow, &record.target);
    let blocked = record.incomplete_attempts == 2
        && record
            .result
            .as_ref()
            .is_some_and(|result| result.incomplete);
    Ok(CommandResult {
        ok: !blocked, state: workflow.phase.clone(), exit_code: if blocked { 8 } else { 0 }, change_id: Some(change_id.to_string()), next: Some(format!("sdd {command} --change {change_id} --result-json '<JSON>'")),
        data: Some(json!({"findings":open})), rendered: None, warnings: None, error: blocked.then(|| SddError::new("E_QUALITY_FAILED", "独立审查两次未完整返回，请恢复保留的审查行动").to_command_error()),
        action_required: Some(AgentActionRequired::AgentReviewExecution { review_id: record.id, change_id: change_id.to_string(), target: record.target, target_hash: record.target_hash, reviewer: record.reviewer, independent_required: record.independent_required,
            context_pack: format!("{}\n\n{role}\n\n{}\n\nBEGIN_UNTRUSTED_REVIEW_TARGET\n{material}\nEND_UNTRUSTED_REVIEW_TARGET\n\n## 必须复核的发现\n{}\n\n核对当前真实源码及验证输出。通过 resultSchema 回传观察，不代替用户接受问题，不自行修改成果。Core 校验绑定，不能证明派发、用户确认或测试真实性。",crate::assets::COLLABORATION_POLICY,crate::assets::WORKSPACE_POLICY,serde_json::to_string_pretty(&open).expect("发现可序列化")),
            result_schema: crate::schema::schema_value("review-result")?.clone(), result_transport: "inline-json".to_string() }),
    })
}

fn open_findings(reviews: &[ReviewRecord], target: &str, reviewer: &str) -> Vec<Finding> {
    reviews
        .iter()
        .filter(|record| {
            !record.superseded && record.target == target && record.reviewer == reviewer
        })
        .flat_map(|record| record.findings.iter())
        .filter(|finding| finding.status == "OPEN")
        .cloned()
        .collect()
}

pub(crate) fn complete(
    cwd: &str,
    runtime: &RuntimeDocument,
    change_id: &str,
    raw: &str,
) -> Result<Completion, SddError> {
    if raw.len() > 4 * 1024 * 1024 {
        return Err(SddError::new("E_QUALITY_FAILED", "审查结果超过大小上限"));
    }
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| SddError::new("E_QUALITY_FAILED", &error.to_string()))?;
    crate::schema::validate_json("review-result", &value)
        .map_err(|error| SddError::new("E_QUALITY_FAILED", &error.message))?;
    let result: ReviewResult = serde_json::from_value(value).expect("Schema 已验证审查结果");
    let pending = pending_record(runtime, change_id)?;
    if result.review_id != pending.id
        || result.target_hash != pending.target_hash
        || pending.target_hash != target_hash(cwd, runtime, change_id, &pending.target)?
    {
        return Err(SddError::new(
            "E_QUALITY_REQUIRED",
            "审查目标或成果版本不匹配，请恢复当前行动",
        ));
    }
    if policy(runtime, change_id)?.independent_required() && result.mode == "self" {
        return Err(SddError::new(
            "E_QUALITY_FAILED",
            "当前任务需要独立审查；能力缺失须明确 self_fallback 及原因",
        ));
    }
    if (result.mode == "self_fallback") != result.fallback_reason.is_some() {
        return Err(SddError::new(
            "E_QUALITY_FAILED",
            "能力降级须且仅须提供 fallbackReason",
        ));
    }
    if pending.incomplete_attempts > 0
        && pending
            .result
            .as_ref()
            .is_some_and(|previous| previous.mode != result.mode)
    {
        return Err(SddError::new(
            "E_QUALITY_FAILED",
            "已派发审查未完整返回，不能改称能力降级",
        ));
    }
    if result.incomplete && result.mode != "independent" {
        return Err(SddError::new(
            "E_QUALITY_FAILED",
            "不完整重派仅适用于真实独立审查",
        ));
    }
    if result.incomplete && pending.incomplete_attempts == 2 {
        return Err(SddError::new(
            "E_QUALITY_FAILED",
            "重派额度已耗尽；保留行动只接受完整审查恢复结果",
        ));
    }
    if result.incomplete && result.verdict != "NOT_READY" {
        return Err(SddError::new("E_QUALITY_FAILED", "不完整审查不能通过"));
    }
    if let Some(previous) = pending.result.as_ref().filter(|result| !result.incomplete) {
        if previous != &result {
            return Err(SddError::new(
                "E_QUALITY_FAILED",
                "已完成审查不可覆盖；请恢复原命令推进",
            ));
        }
        return Ok(if previous.verdict == "READY" {
            Completion::Ready
        } else {
            Completion::Rejected
        });
    }
    let mut reviews = records(&runtime.changes[change_id])?;
    let open = open_findings(&reviews, &pending.target, &pending.reviewer);
    let observed = result
        .rechecks
        .iter()
        .map(|item| (item.finding_id.as_str(), item.present))
        .collect::<BTreeMap<_, _>>();
    if !result.incomplete
        && (observed.len() != result.rechecks.len()
            || observed.keys().copied().collect::<BTreeSet<_>>()
                != open.iter().map(|finding| finding.id.as_str()).collect())
    {
        return Err(SddError::new(
            "E_QUALITY_FAILED",
            "必须逐项复核 Core 下发的未关闭发现，不得遗漏或伪造标识",
        ));
    }
    let blocking = result
        .findings
        .iter()
        .any(|finding| matches!(finding.severity.as_str(), "critical" | "high"))
        || open.iter().any(|finding| {
            matches!(finding.severity.as_str(), "critical" | "high")
                && observed.get(finding.id.as_str()) == Some(&true)
        });
    if !result.incomplete
        && result.verdict == "NOT_READY"
        && result.findings.is_empty()
        && !blocking
    {
        return Err(SddError::new(
            "E_QUALITY_FAILED",
            "NOT_READY 须包含发现或仍存在的阻断项",
        ));
    }
    if result.verdict == "READY" && blocking {
        return Err(SddError::new(
            "E_QUALITY_FAILED",
            "存在阻断发现，审查不能标记 READY",
        ));
    }
    if !result.incomplete {
        for record in reviews.iter_mut().filter(|record| {
            !record.superseded
                && record.target == pending.target
                && record.reviewer == pending.reviewer
        }) {
            for finding in &mut record.findings {
                if finding.status == "OPEN" && observed.get(finding.id.as_str()) == Some(&false) {
                    finding.status = "RESOLVED".into();
                }
            }
        }
    }
    let record = reviews
        .iter_mut()
        .find(|record| record.id == pending.id)
        .expect("前置校验确认存在");
    if result.incomplete {
        record.incomplete_attempts = (record.incomplete_attempts + 1).min(2);
    }
    let retry = result.incomplete && record.incomplete_attempts == 1;
    record.result = Some(result.clone());
    let offset = record.findings.len();
    record.findings.extend(
        result
            .findings
            .iter()
            .enumerate()
            .map(|(index, finding)| Finding {
                id: format!("{}-F{:03}", pending.id, offset + index + 1),
                severity: finding.severity.clone(),
                message: finding.message.clone(),
                file: finding.file.clone(),
                status: "OPEN".into(),
            }),
    );
    if result.incomplete
        && !retry
        && !record
            .findings
            .iter()
            .any(|finding| finding.id.ends_with("-INCOMPLETE"))
    {
        record.findings.push(Finding {
            id: format!("{}-INCOMPLETE", pending.id),
            severity: "high".into(),
            message: "独立审查两次未完整返回，保留行动等待恢复".into(),
            file: None,
            status: "OPEN".into(),
        });
    }
    let mut risks = runtime.changes[change_id]
        .get("reviewRiskFactors")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut escalated = false;
    for risk in &result.risk_factors {
        if !policy(runtime, change_id)?.risk_factors.contains(risk)
            && !risks.iter().any(|item| item == risk)
        {
            risks.push(json!(risk));
            escalated = true;
        }
    }
    RuntimeStore::new(cwd.to_string()).try_update(|document| {
        if escalated {
            super::change_mut(document, change_id)?.insert("reviewRiskFactors".into(), json!(risks));
            let hash = target_hash(cwd, document, change_id, &pending.target)?;
            let id = format!("REVIEW-{}-{:03}", super::workflow(document, change_id)?.run_id, reviews.len() + 1);
            reviews.push(ReviewRecord {
                id: id.clone(), target: pending.target.clone(), target_hash: hash.clone(),
                reviewer: pending.reviewer.clone(), independent_required: true,
                incomplete_attempts: 0, superseded: false, result: None, findings: Vec::new(),
            });
            super::workflow_mut(document, change_id)?.pending_agent_action = Some(json!({
                "type":"AGENT_REVIEW_EXECUTION", "reviewId":id, "target":pending.target, "targetHash":hash,
            }));
        }
        super::change_mut(document, change_id)?.insert("reviews".into(), serde_json::to_value(&reviews).expect("审查记录可序列化"));
        Ok(())
    })?;
    if escalated {
        Ok(Completion::Escalated)
    } else if result.incomplete {
        Ok(Completion::Incomplete)
    } else if result.verdict == "READY" {
        Ok(Completion::Ready)
    } else {
        Ok(Completion::Rejected)
    }
}

pub(crate) fn current_ready(
    cwd: &str,
    runtime: &RuntimeDocument,
    change_id: &str,
    target: &str,
    reviewer: &str,
) -> Result<bool, SddError> {
    let hash = target_hash(cwd, runtime, change_id, target)?;
    let reviews = records(&runtime.changes[change_id])?;
    Ok(reviews
        .iter()
        .rev()
        .find(|record| {
            !record.superseded
                && record.target == target
                && record.reviewer == reviewer
                && record.target_hash == hash
        })
        .is_some_and(|record| {
            record
                .result
                .as_ref()
                .is_some_and(|result| result.verdict == "READY" && !result.incomplete)
        })
        && !open_findings(&reviews, target, reviewer)
            .iter()
            .any(|finding| matches!(finding.severity.as_str(), "critical" | "high")))
}

pub(crate) fn quality_issues(
    runtime: &RuntimeDocument,
    change_id: &str,
) -> Result<Vec<crate::quality::report::Issue>, SddError> {
    Ok(records(&runtime.changes[change_id])?
        .into_iter()
        .filter(|record| !record.superseded && record.target == "QUALITY")
        .flat_map(|record| record.findings)
        .filter(|finding| finding.status == "OPEN")
        .map(|finding| crate::quality::report::Issue {
            code: finding.id,
            severity: finding.severity,
            message: finding.message,
            file: finding.file,
            category: Some("semantic-review".into()),
            start_line: None,
            end_line: None,
            existing_code: None,
            suggestion_code: None,
            origin: Some("agent-review".into()),
        })
        .collect())
}

pub(crate) fn summaries(runtime: &RuntimeDocument, change_id: &str) -> Result<Value, SddError> {
    Ok(json!(records(&runtime.changes[change_id])?.into_iter().filter(|record| !record.superseded).filter_map(|record| record.result.map(|result| json!({"target":record.target,"reviewer":record.reviewer,"mode":result.mode,"verdict":result.verdict,"summary":result.summary,"fallbackReason":result.fallback_reason}))).collect::<Vec<_>>()))
}

pub(crate) fn warnings(
    runtime: &RuntimeDocument,
    change_id: &str,
) -> Result<Option<Vec<CliWarning>>, SddError> {
    let reviews = records(&runtime.changes[change_id])?;
    let mut warnings = Vec::new();
    if reviews.iter().any(|record| {
        !record.superseded
            && record
                .result
                .as_ref()
                .is_some_and(|result| result.mode == "self_fallback")
    }) {
        warnings.push(CliWarning::new(
            "W_REVIEW_SELF_FALLBACK",
            "宿主能力受限，本次采用主 Agent 自查，未完成独立审查",
        ));
    }
    let notes = reviews
        .iter()
        .filter(|record| !record.superseded && record.target == "SPECIFICATION")
        .flat_map(|record| &record.findings)
        .filter(|finding| {
            finding.status == "OPEN" && matches!(finding.severity.as_str(), "medium" | "low")
        })
        .map(|finding| format!("{}：{}", finding.severity, finding.message))
        .collect::<Vec<_>>();
    if !notes.is_empty() {
        warnings.push(CliWarning::new(
            "W_REVIEW_FINDINGS",
            format!("规格存在非阻断审查发现：{}", notes.join("；")),
        ));
    }
    Ok((!warnings.is_empty()).then_some(warnings))
}

pub(crate) fn invalidate(change: &mut serde_json::Map<String, Value>) -> Result<(), SddError> {
    let mut reviews = records(&Value::Object(change.clone()))?;
    for record in &mut reviews {
        record.superseded = true;
        for finding in &mut record.findings {
            if finding.status == "OPEN" {
                finding.status = "SUPERSEDED".into();
            }
        }
    }
    change.remove("candidate");
    change.remove("reviewRiskFactors");
    if !reviews.is_empty() {
        change["reviews"] = serde_json::to_value(reviews).expect("审查记录可序列化");
    }
    Ok(())
}

/// 已完成结果保留到阶段推进成功，原命令可幂等恢复，不重新派发或覆盖。
pub(crate) fn completed_result(
    runtime: &RuntimeDocument,
    change_id: &str,
) -> Result<Option<String>, SddError> {
    let record = pending_record(runtime, change_id)?;
    Ok(record
        .result
        .filter(|result| !result.incomplete)
        .map(|result| serde_json::to_string(&result).expect("结果可序列化")))
}

pub(crate) fn validate_binding(runtime: &RuntimeDocument, change_id: &str) -> Result<(), SddError> {
    if matches!(
        super::workflow(runtime, change_id)?.phase.as_str(),
        "SPEC_WAITING_REVIEW" | "QUALITY_WAITING_REVIEW"
    ) {
        let record = pending_record(runtime, change_id)?;
        if record.independent_required != policy(runtime, change_id)?.independent_required() {
            return Err(SddError::new(
                "E_STATE_CORRUPTED",
                "待处理审查与风险要求不一致",
            ));
        }
    }
    Ok(())
}

pub(crate) fn feedback(runtime: &RuntimeDocument, change_id: &str) -> Result<Value, SddError> {
    let change = &runtime.changes[change_id];
    Ok(
        json!({"candidate": change.get("candidate"), "reviews": summaries(runtime, change_id)?, "findings": records(change)?.iter().filter(|record| !record.superseded && record.target == "SPECIFICATION").flat_map(|record| &record.findings).filter(|finding| finding.status == "OPEN").collect::<Vec<_>>() }),
    )
}

// 绑定确定性报告内容；语义附录由当前 review 生成，排除它以避免 hash 自引用。
fn assessment_binding(change: &Value) -> Value {
    let Some(report) = change.pointer("/reports/quality") else {
        return Value::Null;
    };
    let mut minimality = report["minimality"].clone();
    if let Some(object) = minimality.as_object_mut() {
        object.remove("semanticReviews");
    }
    let issues = report["issues"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|issue| issue["origin"] != "agent-review")
        .cloned()
        .collect::<Vec<_>>();
    json!({"kind":report["kind"],"changeId":report["changeId"],"issues":issues,"minimality":minimality})
}
