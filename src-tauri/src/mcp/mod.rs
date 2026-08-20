//! MCP (Model Context Protocol) 服务器管理模块
//!
//! 本模块负责 MCP 服务器配置的验证、同步和导入导出。
//!
//! ## 模块结构
//!
//! - `validation` - 服务器配置验证
//! - `claude` - Claude MCP 同步和导入
//! - `codex` - Codex MCP 同步和导入（含 TOML 转换）
//! - `gemini` - Gemini MCP 同步和导入
//! - `opencode` - OpenCode MCP 同步和导入（含 local/remote 格式转换）
//! - `hermes` - Hermes MCP 同步和导入

mod claude;
mod codex;
mod gemini;
mod grokbuild;
mod hermes;
mod opencode;
mod validation;

// 重新导出公共 API
pub use claude::{
    import_from_claude, remove_server_from_claude, sync_enabled_to_claude,
    sync_single_server_to_claude,
};
pub use codex::{
    capture_codex_mcp_live_snapshot_for, import_from_codex, import_from_codex_for,
    is_codex_desktop_runtime_mcp_item, is_codex_desktop_runtime_mcp_spec, remove_server_from_codex,
    sync_enabled_to_codex, sync_single_server_to_codex, write_codex_mcp_projection_for,
    CodexMcpLiveSnapshot,
};
pub(crate) use codex::{ensure_codex_cli_mcp_write_allowed, should_sync_codex_mcp};
pub use gemini::{
    import_from_gemini, remove_server_from_gemini, sync_enabled_to_gemini,
    sync_single_server_to_gemini,
};
pub use grokbuild::{
    import_from_grokbuild, remove_server_from_grokbuild, sync_single_server_to_grokbuild,
};
pub use hermes::{import_from_hermes, remove_server_from_hermes, sync_single_server_to_hermes};
pub use opencode::{
    import_from_opencode, remove_server_from_opencode, sync_single_server_to_opencode,
};
