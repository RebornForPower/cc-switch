use indexmap::IndexMap;
use std::collections::HashMap;

use crate::app_config::{AppType, McpServer};
use crate::database::Database;
use crate::error::AppError;
use crate::mcp;
use crate::store::AppState;

/// MCP 相关业务逻辑（v3.7.0 统一结构）
pub struct McpService;

/// Opaque snapshot captured before a Codex CLI provider rewrites live config.
///
/// Consuming this token after the rewrite guarantees MCP ownership is based on
/// the pre-write CLI/Desktop files rather than the newly replaced CLI file.
#[derive(Debug)]
pub struct CodexMcpLiveRewritePreflight {
    servers: IndexMap<String, McpServer>,
    preserved_live: crate::mcp::CodexMcpLiveSnapshot,
    legacy_cli_flags_to_clear: Vec<String>,
}

impl McpService {
    /// 获取所有 MCP 服务器（统一结构）
    pub fn get_all_servers(state: &AppState) -> Result<IndexMap<String, McpServer>, AppError> {
        state.db.get_all_mcp_servers()
    }

    /// 添加或更新 MCP 服务器
    pub fn upsert_server(state: &AppState, server: McpServer) -> Result<(), AppError> {
        if server.apps.codex {
            crate::mcp::ensure_codex_cli_mcp_write_allowed(&server.id, &server.server)?;
        }

        // 读取旧状态：用于处理“编辑时取消勾选某个应用”的场景（需要从对应 live 配置中移除）
        let prev_apps = state
            .db
            .get_all_mcp_servers()?
            .get(&server.id)
            .map(|s| s.apps.clone())
            .unwrap_or_default();

        state.db.save_mcp_server(&server)?;

        // 处理禁用：若旧版本启用但新版本取消，则需要从该应用的 live 配置移除
        if prev_apps.claude && !server.apps.claude {
            Self::remove_server_from_app(state, &server.id, &AppType::Claude)?;
        }
        if prev_apps.codex && !server.apps.codex {
            Self::remove_server_from_app(state, &server.id, &AppType::Codex)?;
        }
        if prev_apps.gemini && !server.apps.gemini {
            Self::remove_server_from_app(state, &server.id, &AppType::Gemini)?;
        }
        if prev_apps.grokbuild && !server.apps.grokbuild {
            Self::remove_server_from_app(state, &server.id, &AppType::GrokBuild)?;
        }
        if prev_apps.opencode && !server.apps.opencode {
            Self::remove_server_from_app(state, &server.id, &AppType::OpenCode)?;
        }
        if prev_apps.hermes && !server.apps.hermes {
            Self::remove_server_from_app(state, &server.id, &AppType::Hermes)?;
        }

        // 同步到各个启用的应用
        Self::sync_server_to_apps(state, &server)?;

        Ok(())
    }

    /// 删除 MCP 服务器
    pub fn delete_server(state: &AppState, id: &str) -> Result<bool, AppError> {
        let server = state.db.get_all_mcp_servers()?.shift_remove(id);

        if let Some(server) = server {
            state.db.delete_mcp_server(id)?;

            // 从所有应用的 live 配置中移除
            Self::remove_server_from_all_apps(state, id, &server)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 切换指定应用的启用状态
    pub fn toggle_app(
        state: &AppState,
        server_id: &str,
        app: AppType,
        enabled: bool,
    ) -> Result<(), AppError> {
        if enabled && app == AppType::Codex {
            let server = state.db.get_all_mcp_servers()?.get(server_id).cloned();
            if let Some(server) = server {
                crate::mcp::ensure_codex_cli_mcp_write_allowed(&server.id, &server.server)?;
            }
        }

        if let Some(server) = state
            .db
            .update_mcp_server_app_enabled(server_id, &app, enabled)?
        {
            // 同步到对应应用
            if enabled {
                Self::sync_server_to_app(state, &server, &app)?;
            } else {
                Self::remove_server_from_app(state, server_id, &app)?;
            }
        }

        Ok(())
    }

    /// 将 MCP 服务器同步到所有启用的应用
    fn sync_server_to_apps(_state: &AppState, server: &McpServer) -> Result<(), AppError> {
        for app in server.apps.enabled_apps() {
            Self::sync_server_to_app_no_config(server, &app)?;
        }

        Ok(())
    }

    /// 将 MCP 服务器同步到指定应用
    fn sync_server_to_app(
        _state: &AppState,
        server: &McpServer,
        app: &AppType,
    ) -> Result<(), AppError> {
        Self::sync_server_to_app_no_config(server, app)
    }

    fn sync_server_to_app_no_config(server: &McpServer, app: &AppType) -> Result<(), AppError> {
        match app {
            AppType::Claude => {
                mcp::sync_single_server_to_claude(&Default::default(), &server.id, &server.server)?;
            }
            AppType::ClaudeDesktop => {
                log::debug!("Claude Desktop 3P profiles do not use CC Switch MCP sync, skipping");
            }
            AppType::Codex => {
                // Codex uses TOML format, must use the correct function
                mcp::sync_single_server_to_codex(&Default::default(), &server.id, &server.server)?;
            }
            AppType::CodexDesktop => {
                // Codex Desktop's runtime owns its MCP configuration; it is
                // deliberately outside CC Switch's CLI projection.
                log::debug!("Codex Desktop MCP is runtime-managed, skipping sync");
            }
            AppType::Gemini => {
                mcp::sync_single_server_to_gemini(&Default::default(), &server.id, &server.server)?;
            }
            AppType::GrokBuild => {
                mcp::sync_single_server_to_grokbuild(
                    &Default::default(),
                    &server.id,
                    &server.server,
                )?;
            }
            AppType::OpenCode => {
                mcp::sync_single_server_to_opencode(
                    &Default::default(),
                    &server.id,
                    &server.server,
                )?;
            }
            AppType::OpenClaw => {
                // OpenClaw MCP support is still in development (Issue #4834)
                // Skip for now
                log::debug!("OpenClaw MCP support is still in development, skipping sync");
            }
            AppType::Hermes => {
                mcp::sync_single_server_to_hermes(&Default::default(), &server.id, &server.server)?;
            }
            AppType::Pi => {}
        }
        Ok(())
    }

    /// 从所有曾启用过该服务器的应用中移除
    fn remove_server_from_all_apps(
        state: &AppState,
        id: &str,
        server: &McpServer,
    ) -> Result<(), AppError> {
        // 从所有曾启用的应用中移除
        for app in server.apps.enabled_apps() {
            Self::remove_server_from_app(state, id, &app)?;
        }
        Ok(())
    }

    fn remove_server_from_app(_state: &AppState, id: &str, app: &AppType) -> Result<(), AppError> {
        Self::remove_server_from_app_no_config(id, app)
    }

    fn remove_server_from_app_no_config(id: &str, app: &AppType) -> Result<(), AppError> {
        match app {
            AppType::Claude => mcp::remove_server_from_claude(id)?,
            AppType::ClaudeDesktop => {
                log::debug!("Claude Desktop 3P profiles do not use CC Switch MCP sync, skipping");
            }
            AppType::Codex => mcp::remove_server_from_codex(id)?,
            AppType::CodexDesktop => {
                // Codex Desktop's runtime owns its MCP configuration.
                log::debug!("Codex Desktop MCP is runtime-managed, skipping remove");
            }
            AppType::Gemini => mcp::remove_server_from_gemini(id)?,
            AppType::GrokBuild => mcp::remove_server_from_grokbuild(id)?,
            AppType::OpenCode => {
                mcp::remove_server_from_opencode(id)?;
            }
            AppType::OpenClaw => {
                // OpenClaw MCP support is still in development
                log::debug!("OpenClaw MCP support is still in development, skipping remove");
            }
            AppType::Hermes => {
                mcp::remove_server_from_hermes(id)?;
            }
            AppType::Pi => {}
        }
        Ok(())
    }

    /// 手动同步所有启用的 MCP 服务器到对应的应用。
    ///
    /// Best-effort：单个应用投影失败（如 ~/.claude.json 坏 JSON）不阻断
    /// 其余应用——各应用的 live 文件互相独立，一处损坏没有理由让其他
    /// 应用的 MCP 状态陈旧。全部跑完后若有失败，聚合成一个错误上报，
    /// 保留调用方的可见性。
    pub fn sync_all_enabled(state: &AppState) -> Result<(), AppError> {
        let servers = Self::get_all_servers(state)?;

        let mut failures: Vec<String> = Vec::new();
        for app in AppType::all() {
            // Codex needs a live-file ownership check before projection. Keep
            // it inside Codex's own iteration so a conflict cannot prevent
            // Claude/Gemini/etc. from completing their independent sync.
            let result = if app == AppType::Codex && !crate::mcp::should_sync_codex_mcp() {
                // Match the existing per-server MCP behavior: a missing CLI
                // directory means Codex CLI has not been initialized, so a
                // global sync must not create it while syncing another app.
                Ok(())
            } else if app == AppType::Codex {
                Self::preflight_codex_live_rewrite(state)
                    .and_then(|preflight| Self::sync_codex_after_live_rewrite(state, preflight))
            } else {
                Self::project_servers_to_app(state, &servers, &app)
            };
            if let Err(err) = result {
                log::warn!("同步 MCP 到 {app:?} 失败: {err}");
                failures.push(format!("{}: {err}", app.as_str()));
            }
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(AppError::Message(format!(
                "部分应用 MCP 同步失败: {}",
                failures.join("; ")
            )))
        }
    }

    /// 只把启用状态投影到单个应用。某个应用的 live 被整体重写后用它做
    /// 定向重投影，避免把无关应用的失败面（如 ~/.claude.json 坏 JSON）
    /// 牵连进目标应用的关键路径。
    ///
    /// Codex provider rewrites must call `preflight_codex_live_rewrite` before
    /// writing and `sync_codex_after_live_rewrite` afterwards. This generic
    /// helper intentionally does not infer ownership from post-write files.
    pub fn sync_enabled_for_app(state: &AppState, app: &AppType) -> Result<(), AppError> {
        if matches!(
            app,
            AppType::OpenClaw | AppType::ClaudeDesktop | AppType::CodexDesktop | AppType::Pi
        ) {
            return Ok(());
        }
        if matches!(app, AppType::Codex) {
            if !crate::mcp::should_sync_codex_mcp() {
                return Ok(());
            }
            let preflight = Self::preflight_codex_live_rewrite(state)?;
            return Self::sync_codex_after_live_rewrite(state, preflight);
        }
        let servers = Self::get_all_servers(state)?;
        Self::project_servers_to_app(state, &servers, app)
    }

    /// Inspect Codex MCP ownership before a provider replaces CLI
    /// `config.toml`, then capture the authoritative CLI projection.
    ///
    /// Entries owned by the Desktop runtime or another external writer are
    /// retained separately from the authoritative CLI database projection.
    pub fn preflight_codex_live_rewrite(
        state: &AppState,
    ) -> Result<CodexMcpLiveRewritePreflight, AppError> {
        Self::preflight_codex_live_rewrite_with_db(state.db.as_ref())
    }

    /// Database-only variant used by live writers that do not own an
    /// `AppState` (for example proxy takeover restore).
    pub fn preflight_codex_live_rewrite_with_db(
        db: &Database,
    ) -> Result<CodexMcpLiveRewritePreflight, AppError> {
        let servers = db.get_all_mcp_servers()?;
        let preserved_live =
            crate::mcp::capture_codex_mcp_live_snapshot_for(crate::codex_config::CodexTarget::Cli)?;
        Self::build_codex_live_rewrite_preflight(servers, preserved_live)
    }

    /// Restore-only variant of [`Self::preflight_codex_live_rewrite_with_db`].
    ///
    /// A damaged `config.toml` must not prevent proxy shutdown from restoring
    /// the provider/auth state saved in the takeover backup.  The normal
    /// preflight remains strict so ordinary provider writes still surface the
    /// malformed TOML instead of silently replacing it.  During restore, when
    /// the only failure is parsing the current Codex config, use an empty live
    /// MCP snapshot and project the database-owned CLI entries after the
    /// backup has been written.  Non-parse errors (I/O, database, path
    /// isolation, etc.) continue to fail the restore.
    pub fn preflight_codex_live_rewrite_for_restore_with_db(
        db: &Database,
    ) -> Result<CodexMcpLiveRewritePreflight, AppError> {
        match Self::preflight_codex_live_rewrite_with_db(db) {
            Ok(preflight) => Ok(preflight),
            Err(error) if Self::is_codex_live_config_parse_error(&error) => {
                log::warn!(
                    "Codex Live config.toml 无法解析，恢复时将使用空 MCP 快照并重建 CLI MCP: {error}"
                );
                Self::build_codex_live_rewrite_preflight(
                    db.get_all_mcp_servers()?,
                    crate::mcp::CodexMcpLiveSnapshot::default(),
                )
            }
            Err(error) => Err(error),
        }
    }

    fn is_codex_live_config_parse_error(error: &AppError) -> bool {
        match error {
            // `read_and_validate_codex_config_text_for` reports TOML syntax
            // errors through this typed variant.
            AppError::Toml { .. } => true,
            // `toml_edit` can reject text that the `toml` validator accepted;
            // the capture helper wraps that second parse in McpValidation.
            AppError::McpValidation(message) => message.starts_with("解析 Codex config.toml 失败:"),
            _ => false,
        }
    }

    fn build_codex_live_rewrite_preflight(
        servers: IndexMap<String, McpServer>,
        mut preserved_live: crate::mcp::CodexMcpLiveSnapshot,
    ) -> Result<CodexMcpLiveRewritePreflight, AppError> {
        let shared_directory = crate::codex_config::codex_config_dirs_conflict();
        let live_desktop_runtime_ids = preserved_live
            .iter()
            .filter_map(|(id, item)| {
                crate::mcp::is_codex_desktop_runtime_mcp_item(id, item).then_some(id.to_string())
            })
            .collect::<std::collections::HashSet<_>>();

        let legacy_cli_flags_to_clear = servers
            .values()
            .filter_map(|server| {
                (server.apps.codex
                    && (crate::mcp::is_codex_desktop_runtime_mcp_spec(&server.id, &server.server)
                        || live_desktop_runtime_ids.contains(&server.id)))
                .then_some(server.id.clone())
            })
            .collect::<Vec<_>>();
        let mut projected_servers = servers.clone();
        for id in &legacy_cli_flags_to_clear {
            if let Some(server) = projected_servers.get_mut(id) {
                server.apps.codex = false;
            }
        }

        preserved_live.retain(|id, item| {
            let desktop_runtime = crate::mcp::is_codex_desktop_runtime_mcp_item(id, item);
            if desktop_runtime {
                // A runtime entry in an isolated CLI directory is historical
                // contamination and should disappear on the next CLI rewrite.
                return shared_directory;
            }
            match servers.get(id) {
                Some(server) if server.apps.codex => false,
                // In a shared file a DB-disabled ID can still belong to
                // Desktop. Direct CLI disable already removes its own entry.
                Some(_) => false,
                None => true,
            }
        });

        Ok(CodexMcpLiveRewritePreflight {
            servers: projected_servers,
            preserved_live,
            legacy_cli_flags_to_clear,
        })
    }

    /// Re-project Codex CLI MCP after a provider live rewrite using only the
    /// preflight snapshot. No CLI/Desktop live file is read for ownership here.
    pub fn sync_codex_after_live_rewrite(
        state: &AppState,
        preflight: CodexMcpLiveRewritePreflight,
    ) -> Result<(), AppError> {
        Self::sync_codex_after_live_rewrite_with_db(state.db.as_ref(), preflight)
    }

    /// Database-only counterpart for callers that perform the live rewrite
    /// without an `AppState`. The preflight token remains the sole projection
    /// source; the database and live files are deliberately not re-read.
    pub fn sync_codex_after_live_rewrite_with_db(
        db: &Database,
        preflight: CodexMcpLiveRewritePreflight,
    ) -> Result<(), AppError> {
        crate::mcp::write_codex_mcp_projection_for(
            crate::codex_config::CodexTarget::Cli,
            &preflight.preserved_live,
            &preflight.servers,
        )?;
        Self::finalize_legacy_codex_desktop_runtime_mcp(db, &preflight.legacy_cli_flags_to_clear)
    }

    fn project_servers_to_app(
        _state: &AppState,
        servers: &IndexMap<String, McpServer>,
        app: &AppType,
    ) -> Result<(), AppError> {
        Self::project_servers_to_app_no_state(servers, app)
    }

    fn project_servers_to_app_no_state(
        servers: &IndexMap<String, McpServer>,
        app: &AppType,
    ) -> Result<(), AppError> {
        if matches!(
            app,
            AppType::OpenClaw | AppType::ClaudeDesktop | AppType::CodexDesktop | AppType::Pi
        ) {
            return Ok(());
        }

        for server in servers.values() {
            if server.apps.is_enabled_for(app) {
                Self::sync_server_to_app_no_config(server, app)?;
            } else {
                Self::remove_server_from_app_no_config(&server.id, app)?;
            }
        }

        Ok(())
    }

    // ========================================================================
    // 兼容层：支持旧的 v3.6.x 命令（已废弃，将在 v4.0 移除）
    // ========================================================================

    /// [已废弃] 获取指定应用的 MCP 服务器（兼容旧 API）
    #[deprecated(since = "3.7.0", note = "Use get_all_servers instead")]
    pub fn get_servers(
        state: &AppState,
        app: AppType,
    ) -> Result<HashMap<String, serde_json::Value>, AppError> {
        let all_servers = Self::get_all_servers(state)?;
        let mut result = HashMap::new();

        for (id, server) in all_servers {
            if server.apps.is_enabled_for(&app) {
                result.insert(id, server.server);
            }
        }

        Ok(result)
    }

    /// [已废弃] 设置 MCP 服务器在指定应用的启用状态（兼容旧 API）
    #[deprecated(since = "3.7.0", note = "Use toggle_app instead")]
    pub fn set_enabled(
        state: &AppState,
        app: AppType,
        id: &str,
        enabled: bool,
    ) -> Result<bool, AppError> {
        Self::toggle_app(state, id, app, enabled)?;
        Ok(true)
    }

    /// [已废弃] 同步启用的 MCP 到指定应用（兼容旧 API）
    #[deprecated(since = "3.7.0", note = "Use sync_all_enabled instead")]
    pub fn sync_enabled(state: &AppState, app: AppType) -> Result<(), AppError> {
        let servers = Self::get_all_servers(state)?;

        for server in servers.values() {
            if server.apps.is_enabled_for(&app) {
                Self::sync_server_to_app(state, server, &app)?;
            }
        }

        Ok(())
    }

    /// 从 Claude 导入 MCP（v3.7.0 已更新为统一结构）
    pub fn import_from_claude(state: &AppState) -> Result<usize, AppError> {
        // 创建临时 MultiAppConfig 用于导入
        let mut temp_config = crate::app_config::MultiAppConfig::default();

        // 调用原有的导入逻辑（从 mcp.rs）
        let count = crate::mcp::import_from_claude(&mut temp_config)?;

        let mut new_count = 0;

        // 如果有导入的服务器，保存到数据库
        if count > 0 {
            if let Some(servers) = &temp_config.mcp.servers {
                let mut existing = state.db.get_all_mcp_servers()?;
                for server in servers.values() {
                    // 已存在：仅启用 Claude，不覆盖其他字段（与导入模块语义保持一致）
                    let to_save = if let Some(existing_server) = existing.get(&server.id) {
                        let mut merged = existing_server.clone();
                        merged.apps.claude = true;
                        merged
                    } else {
                        // 真正的新服务器
                        new_count += 1;
                        server.clone()
                    };

                    state.db.save_mcp_server(&to_save)?;
                    existing.insert(to_save.id.clone(), to_save.clone());

                    // 导入是读取已有配置，不应反向写回任何应用的 live 配置。
                    // 显式编辑、启用/禁用或手动同步时再执行写回。
                }
            }
        }

        Ok(new_count)
    }

    /// 从 Codex 导入 MCP（v3.7.0 已更新为统一结构）
    pub fn import_from_codex(state: &AppState) -> Result<usize, AppError> {
        // Import is read-only for the live file, but a shared CLI/Desktop
        // file is still ambiguous when it contains MCP.  Reuse the same
        // preflight as rewrite paths: an empty shared file remains compatible
        // with the historical default directory, while an MCP-bearing one is
        // rejected before any database row is changed.
        let preflight = Self::preflight_codex_live_rewrite(state)?;

        // 创建临时 MultiAppConfig 用于导入
        let mut temp_config = crate::app_config::MultiAppConfig::default();

        // 调用原有的导入逻辑（从 mcp.rs）
        let count = crate::mcp::import_from_codex_for(
            &mut temp_config,
            crate::codex_config::CodexTarget::Cli,
        )?;

        // Import is read-only for live files. Commit the historical cleanup
        // only after the CLI file parsed successfully; a failed read leaves
        // the database untouched.
        Self::finalize_legacy_codex_desktop_runtime_mcp(
            state.db.as_ref(),
            &preflight.legacy_cli_flags_to_clear,
        )?;

        let mut new_count = 0;

        // 如果有导入的服务器，保存到数据库
        if count > 0 {
            if let Some(servers) = &temp_config.mcp.servers {
                let mut existing = state.db.get_all_mcp_servers()?;
                for server in servers.values() {
                    // 已存在：仅启用 Codex，不覆盖其他字段（与导入模块语义保持一致）
                    let to_save = if let Some(existing_server) = existing.get(&server.id) {
                        let mut merged = existing_server.clone();
                        merged.apps.codex = true;
                        merged
                    } else {
                        // 真正的新服务器
                        new_count += 1;
                        server.clone()
                    };

                    state.db.save_mcp_server(&to_save)?;
                    existing.insert(to_save.id.clone(), to_save.clone());

                    // 导入是读取已有配置，不应反向写回任何应用的 live 配置。
                    // 显式编辑、启用/禁用或手动同步时再执行写回。
                }
            }
        }

        Ok(new_count)
    }

    fn finalize_legacy_codex_desktop_runtime_mcp(
        db: &Database,
        ids: &[String],
    ) -> Result<(), AppError> {
        for id in ids {
            log::info!("清理 Codex Desktop runtime MCP '{id}' 的历史 CLI 启用状态");
            let _ = db.update_mcp_server_app_enabled(id, &AppType::Codex, false)?;
        }
        Ok(())
    }

    /// 从 Gemini 导入 MCP（v3.7.0 已更新为统一结构）
    pub fn import_from_gemini(state: &AppState) -> Result<usize, AppError> {
        // 创建临时 MultiAppConfig 用于导入
        let mut temp_config = crate::app_config::MultiAppConfig::default();

        // 调用原有的导入逻辑（从 mcp.rs）
        let count = crate::mcp::import_from_gemini(&mut temp_config)?;

        let mut new_count = 0;

        // 如果有导入的服务器，保存到数据库
        if count > 0 {
            if let Some(servers) = &temp_config.mcp.servers {
                let mut existing = state.db.get_all_mcp_servers()?;
                for server in servers.values() {
                    // 已存在：仅启用 Gemini，不覆盖其他字段（与导入模块语义保持一致）
                    let to_save = if let Some(existing_server) = existing.get(&server.id) {
                        let mut merged = existing_server.clone();
                        merged.apps.gemini = true;
                        merged
                    } else {
                        // 真正的新服务器
                        new_count += 1;
                        server.clone()
                    };

                    state.db.save_mcp_server(&to_save)?;
                    existing.insert(to_save.id.clone(), to_save.clone());

                    // 导入是读取已有配置，不应反向写回任何应用的 live 配置。
                    // 显式编辑、启用/禁用或手动同步时再执行写回。
                }
            }
        }

        Ok(new_count)
    }

    /// 从 Grok Build 的 `[mcp_servers]` 导入 MCP。
    pub fn import_from_grokbuild(state: &AppState) -> Result<usize, AppError> {
        let mut temp_config = crate::app_config::MultiAppConfig::default();
        let count = crate::mcp::import_from_grokbuild(&mut temp_config)?;
        let mut new_count = 0;

        if count > 0 {
            if let Some(servers) = &temp_config.mcp.servers {
                let mut existing = state.db.get_all_mcp_servers()?;
                for server in servers.values() {
                    let to_save = if let Some(existing_server) = existing.get(&server.id) {
                        let mut merged = existing_server.clone();
                        merged.apps.grokbuild = true;
                        merged
                    } else {
                        new_count += 1;
                        server.clone()
                    };
                    state.db.save_mcp_server(&to_save)?;
                    existing.insert(to_save.id.clone(), to_save);
                }
            }
        }
        Ok(new_count)
    }

    /// 从 OpenCode 导入 MCP（v3.9.2+ 新增）
    pub fn import_from_opencode(state: &AppState) -> Result<usize, AppError> {
        // 创建临时 MultiAppConfig 用于导入
        let mut temp_config = crate::app_config::MultiAppConfig::default();

        // 调用原有的导入逻辑（从 mcp/opencode.rs）
        let count = crate::mcp::import_from_opencode(&mut temp_config)?;

        let mut new_count = 0;

        // 如果有导入的服务器，保存到数据库
        if count > 0 {
            if let Some(servers) = &temp_config.mcp.servers {
                let mut existing = state.db.get_all_mcp_servers()?;
                for server in servers.values() {
                    // 已存在：仅启用 OpenCode，不覆盖其他字段（与导入模块语义保持一致）
                    let to_save = if let Some(existing_server) = existing.get(&server.id) {
                        let mut merged = existing_server.clone();
                        merged.apps.opencode = true;
                        merged
                    } else {
                        // 真正的新服务器
                        new_count += 1;
                        server.clone()
                    };

                    state.db.save_mcp_server(&to_save)?;
                    existing.insert(to_save.id.clone(), to_save.clone());

                    // 导入是读取已有配置，不应反向写回任何应用的 live 配置。
                    // 显式编辑、启用/禁用或手动同步时再执行写回。
                }
            }
        }

        Ok(new_count)
    }

    /// 从 Hermes 导入 MCP
    pub fn import_from_hermes(state: &AppState) -> Result<usize, AppError> {
        // 创建临时 MultiAppConfig 用于导入
        let mut temp_config = crate::app_config::MultiAppConfig::default();

        // 调用导入逻辑（从 mcp/hermes.rs）
        let count = crate::mcp::import_from_hermes(&mut temp_config)?;

        let mut new_count = 0;

        // 如果有导入的服务器，保存到数据库
        if count > 0 {
            if let Some(servers) = &temp_config.mcp.servers {
                let mut existing = state.db.get_all_mcp_servers()?;
                for server in servers.values() {
                    // 已存在：仅启用 Hermes，不覆盖其他字段（与导入模块语义保持一致）
                    let to_save = if let Some(existing_server) = existing.get(&server.id) {
                        let mut merged = existing_server.clone();
                        merged.apps.hermes = true;
                        merged
                    } else {
                        // 真正的新服务器
                        new_count += 1;
                        server.clone()
                    };

                    state.db.save_mcp_server(&to_save)?;
                    existing.insert(to_save.id.clone(), to_save.clone());

                    // 导入是读取已有配置，不应反向写回任何应用的 live 配置。
                    // 显式编辑、启用/禁用或手动同步时再执行写回。
                }
            }
        }

        Ok(new_count)
    }

    /// 从所有支持 MCP 的应用导入服务器，返回新导入的数量。
    ///
    /// Best-effort：单个应用导入失败（如坏 config.toml）不阻断其余应用；
    /// 全部跑完后若有失败，聚合成一个错误上报——历史实现逐应用
    /// `unwrap_or(0)` 吞错，坏文件只会表现为"导入成功 0 个"，用户
    /// 无从得知哪个应用出了问题。
    pub fn import_from_all_apps(state: &AppState) -> Result<usize, AppError> {
        let mut total = 0;
        let mut failures: Vec<String> = Vec::new();

        let results: [(&str, Result<usize, AppError>); 6] = [
            ("claude", Self::import_from_claude(state)),
            ("codex", Self::import_from_codex(state)),
            ("gemini", Self::import_from_gemini(state)),
            ("grokbuild", Self::import_from_grokbuild(state)),
            ("opencode", Self::import_from_opencode(state)),
            ("hermes", Self::import_from_hermes(state)),
        ];
        for (app, result) in results {
            match result {
                Ok(count) => total += count,
                Err(err) => {
                    log::warn!("从 {app} 导入 MCP 失败: {err}");
                    failures.push(format!("{app}: {err}"));
                }
            }
        }

        if failures.is_empty() {
            Ok(total)
        } else {
            Err(AppError::Message(format!(
                "已导入 {total} 个，部分应用导入失败: {}",
                failures.join("; ")
            )))
        }
    }
}
