//! sdd-core：SDD 状态机与质量门禁执行层。
//!
//! Core 是整个工作流的统一调度入口，所有平台适配器最终都只通过这里
//! 推进状态机、写入制品和返回结果。

mod assets;
pub mod commands;
pub mod contracts;
pub mod engines;
pub mod error;
pub mod git;
pub mod knowledge;
pub mod policies;
pub mod protocol;
pub mod quality;
mod safe_fs;
pub mod schema;
pub mod security;
pub mod state;
mod subprocess;

use contracts::{CommandRequest, CommandResult};
use error::SddError;

/// 统一调度入口：解析命令并分发到对应实现。
pub fn run(request: &CommandRequest) -> Result<CommandResult, SddError> {
    let cwd = &request.cwd;
    match request.command.as_str() {
        // status 是纯只读命令，不依赖完整分发流程
        "status" => commands::status::run_status(cwd, request.args.as_ref()),
        "codebase" => commands::codebase::run_codebase(cwd, request.args.as_ref()),
        "change" => commands::change::run_change(cwd, request.args.as_ref()),
        "init" => commands::init::run_init(cwd, request.args.as_ref()),
        "spec" => commands::spec::run_spec(cwd, request.args.as_ref()),
        "plan" => commands::plan::run_plan(cwd, request.args.as_ref()),
        "build" => commands::build::run_build(cwd, request.args.as_ref()),
        "verify" => commands::verify::run_verify(cwd, request.args.as_ref()),
        "archive" => commands::archive::run_archive(cwd, request.args.as_ref()),
        _ => Err(SddError::new(
            "E_INVALID_PHASE_COMMAND",
            &format!("命令 {} 不可用", request.command),
        )),
    }
}
