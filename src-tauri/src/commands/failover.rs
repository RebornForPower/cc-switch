//! 故障转移队列命令
//!
//! 管理代理模式下的故障转移队列（基于 providers 表的 in_failover_queue 字段）

use crate::database::FailoverQueueItem;
use crate::provider::Provider;
use crate::store::AppState;
use std::str::FromStr;
use tauri::Emitter;

fn require_failover_app(app_type: &str) -> Result<(), String> {
    let app = crate::app_config::AppType::from_str(app_type)
        .map_err(|error| format!("无效的应用类型: {error}"))?;
    if !app.supports_failover() {
        return Err(format!("{} 不支持故障转移", app.as_str()));
    }
    Ok(())
}

fn require_failover_provider(
    db: &crate::database::Database,
    app_type: &str,
    provider_id: &str,
) -> Result<Provider, String> {
    let provider = db
        .get_provider_by_id(provider_id, app_type)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("供应商不存在: {provider_id}"))?;
    if !crate::proxy::provider_router::provider_supports_failover(app_type, &provider) {
        return Err("该供应商不支持自动故障转移".to_string());
    }
    Ok(provider)
}

fn get_filtered_failover_queue(
    db: &crate::database::Database,
    app_type: &str,
) -> Result<Vec<FailoverQueueItem>, String> {
    let queue = db
        .get_failover_queue(app_type)
        .map_err(|error| error.to_string())?;
    let providers = db
        .get_all_providers(app_type)
        .map_err(|error| error.to_string())?;
    Ok(queue
        .into_iter()
        .filter(|item| {
            providers.get(&item.provider_id).is_some_and(|provider| {
                crate::proxy::provider_router::provider_supports_failover(app_type, provider)
            })
        })
        .collect())
}

/// Keep the logical Desktop target inside the queue while its independent
/// failover switch is enabled. The router only considers queued providers, so
/// removing the current target would leave the persisted current provider
/// unreachable on the next request.
fn ensure_failover_queue_removal_allowed(
    app_type: &str,
    provider_id: &str,
    auto_failover_enabled: bool,
    current_provider_id: Option<&str>,
) -> Result<(), String> {
    if app_type == crate::app_config::AppType::CodexDesktop.as_str()
        && auto_failover_enabled
        && current_provider_id == Some(provider_id)
    {
        return Err(
            "无法移除当前 Codex Desktop 供应商：请先关闭故障转移或切换到其他队列供应商".to_string(),
        );
    }

    Ok(())
}

/// Desktop's route switch starts the shared gateway, so its failover switch
/// is persisted independently and never touches `proxy_config`.
async fn set_codex_desktop_auto_failover_enabled(
    app: &tauri::AppHandle,
    state: &AppState,
    enabled: bool,
) -> Result<(), String> {
    let app_type = crate::app_config::AppType::CodexDesktop.as_str();
    // Keep the same lock order as provider/takeover switching: app lock first,
    // then the shared gateway lifecycle lock. This closes the window where a
    // delayed profile stop could observe the old flag and stop the gateway
    // just after Desktop failover was enabled.
    let _app_guard = state.proxy_service.lock_switch_for_app(app_type).await;
    let _gateway_guard = state.proxy_service.lock_gateway_lifecycle().await;
    let previous_enabled = state
        .db
        .get_codex_desktop_auto_failover_enabled()
        .map_err(|error| error.to_string())?;
    if enabled && !state.proxy_service.is_running().await {
        return Err("需要先启用 Codex Desktop 本地路由，再开启故障转移".to_string());
    }

    // Failover only changes the logical gateway target.  A Direct provider
    // would leave Desktop pointing at its native endpoint, so enabling the
    // queue in that state would report success without routing requests
    // through the gateway.  Require the already-selected provider to be a
    // Proxy provider before allowing the independent switch.
    if enabled {
        let current_id = crate::settings::get_effective_current_provider(
            &state.db,
            &crate::app_config::AppType::CodexDesktop,
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "请先选择 Codex Desktop 的 Proxy 模式供应商".to_string())?;
        require_failover_provider(&state.db, app_type, &current_id)
            .map_err(|_| "请先选择 Codex Desktop 的 Proxy 模式供应商".to_string())?;
    }

    // Read the shared CLI circuit settings before mutating Desktop state, so
    // an unexpected database error cannot leave the independent switch half
    // enabled after its target has already moved.
    let desktop_circuit_config = if enabled {
        Some(
            state
                .db
                .get_proxy_config_for_app(crate::app_config::AppType::Codex.as_str())
                .await
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };

    let mut auto_added_provider_id: Option<String> = None;
    let p1_provider_id = if enabled {
        let mut queue = get_filtered_failover_queue(&state.db, app_type)?;
        if queue.is_empty() {
            let current_id = crate::settings::get_effective_current_provider(
                &state.db,
                &crate::app_config::AppType::CodexDesktop,
            )
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "故障转移队列为空，且未设置当前供应商，无法开启故障转移".to_string())?;
            require_failover_provider(&state.db, app_type, &current_id)?;
            state
                .db
                .add_to_failover_queue(app_type, &current_id)
                .map_err(|error| error.to_string())?;
            auto_added_provider_id = Some(current_id);
            queue = get_filtered_failover_queue(&state.db, app_type)?;
        }
        queue
            .first()
            .map(|item| item.provider_id.clone())
            .ok_or_else(|| "故障转移队列为空，无法开启故障转移".to_string())?
    } else {
        String::new()
    };

    if let Err(error) = state.db.set_codex_desktop_auto_failover_enabled(enabled) {
        if let Some(provider_id) = auto_added_provider_id {
            let _ = state.db.remove_from_failover_queue(app_type, &provider_id);
        }
        return Err(error.to_string());
    }

    if enabled {
        if let Err(error) = state
            .proxy_service
            .switch_failover_target_inner(app_type, &p1_provider_id)
            .await
        {
            let _ = state
                .db
                .set_codex_desktop_auto_failover_enabled(previous_enabled);
            if let Some(provider_id) = auto_added_provider_id {
                let _ = state.db.remove_from_failover_queue(app_type, &provider_id);
            }
            return Err(error);
        }

        // Desktop reuses Codex CLI's circuit settings at runtime. Existing
        // Desktop breakers may have been created before the switch was on,
        // so update them to the same config instead of waiting for a restart.
        if let Some(cli_config) = desktop_circuit_config {
            state
                .proxy_service
                .update_circuit_breaker_config_for_app(
                    app_type,
                    crate::proxy::CircuitBreakerConfig::from(&cli_config),
                )
                .await?;
        }
    }

    if enabled {
        let event_data = serde_json::json!({
            "appType": app_type,
            "providerId": p1_provider_id,
            "source": "failoverEnabled"
        });
        let _ = app.emit("provider-switched", event_data);
    }

    if let Ok(new_menu) = crate::tray::create_tray_menu(app, state) {
        if let Some(tray) = app.tray_by_id(crate::tray::TRAY_ID) {
            let _ = tray.set_menu(Some(new_menu));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_failover_queue_removal_allowed, require_failover_app, require_failover_provider,
    };
    use crate::database::Database;
    use crate::provider::{AuthBinding, AuthBindingSource, Provider, ProviderMeta};
    use serde_json::json;

    #[test]
    fn failover_rejects_apps_without_a_proxy_data_plane() {
        assert!(require_failover_app("claude").is_ok());
        assert!(require_failover_app("codex-desktop").is_ok());
        assert!(require_failover_app("pi").is_err());
    }

    #[test]
    fn failover_rejects_codex_official_account_cards() {
        let db = Database::memory().expect("memory db");
        let mut official = Provider::with_id(
            "official-a".to_string(),
            "OpenAI Official".to_string(),
            json!({ "auth": {}, "config": "" }),
            None,
        );
        official.category = Some("official".to_string());
        official.meta = Some(ProviderMeta {
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some("codex_oauth".to_string()),
                account_id: Some("account-a".to_string()),
            }),
            ..Default::default()
        });
        db.save_provider("codex", &official).expect("save official");

        assert!(require_failover_provider(&db, "codex", &official.id).is_err());
    }

    #[test]
    fn codex_desktop_keeps_current_target_in_queue_while_failover_is_enabled() {
        let db = Database::memory().expect("memory db");
        let primary = Provider::with_id(
            "desktop-primary".to_string(),
            "Desktop Primary".to_string(),
            json!({}),
            None,
        );
        db.save_provider("codex-desktop", &primary)
            .expect("save Desktop provider");
        db.add_to_failover_queue("codex-desktop", &primary.id)
            .expect("queue Desktop provider");

        let error = ensure_failover_queue_removal_allowed(
            "codex-desktop",
            "desktop-primary",
            true,
            Some("desktop-primary"),
        )
        .expect_err("the current Desktop target must remain queued");
        assert!(error.contains("无法移除当前 Codex Desktop 供应商"));
        assert!(db
            .is_in_failover_queue("codex-desktop", &primary.id)
            .expect("read Desktop queue state"));

        assert!(ensure_failover_queue_removal_allowed(
            "codex-desktop",
            "desktop-secondary",
            true,
            Some("desktop-primary"),
        )
        .is_ok());
        assert!(ensure_failover_queue_removal_allowed(
            "codex-desktop",
            "desktop-primary",
            false,
            Some("desktop-primary"),
        )
        .is_ok());
        assert!(ensure_failover_queue_removal_allowed(
            "codex",
            "desktop-primary",
            true,
            Some("desktop-primary"),
        )
        .is_ok());
    }
}

/// 获取故障转移队列
#[tauri::command]
pub async fn get_failover_queue(
    state: tauri::State<'_, AppState>,
    app_type: String,
) -> Result<Vec<FailoverQueueItem>, String> {
    require_failover_app(&app_type)?;
    get_filtered_failover_queue(&state.db, &app_type)
}

/// 获取可添加到故障转移队列的供应商（不在队列中的）
#[tauri::command]
pub async fn get_available_providers_for_failover(
    state: tauri::State<'_, AppState>,
    app_type: String,
) -> Result<Vec<Provider>, String> {
    require_failover_app(&app_type)?;
    let providers = state
        .db
        .get_available_providers_for_failover(&app_type)
        .map_err(|e| e.to_string())?;
    Ok(providers
        .into_iter()
        .filter(|provider| {
            crate::proxy::provider_router::provider_supports_failover(&app_type, provider)
        })
        .collect())
}

/// 添加供应商到故障转移队列
#[tauri::command]
pub async fn add_to_failover_queue(
    state: tauri::State<'_, AppState>,
    app_type: String,
    provider_id: String,
) -> Result<(), String> {
    require_failover_app(&app_type)?;
    require_failover_provider(&state.db, &app_type, &provider_id)?;
    state
        .db
        .add_to_failover_queue(&app_type, &provider_id)
        .map_err(|e| e.to_string())
}

/// 从故障转移队列移除供应商
#[tauri::command]
pub async fn remove_from_failover_queue(
    state: tauri::State<'_, AppState>,
    app_type: String,
    provider_id: String,
) -> Result<(), String> {
    require_failover_app(&app_type)?;

    // Serialize the check and removal with Desktop target switches. Without
    // this shared lock, a concurrent failover could change the current target
    // between the guard below and the database update.
    let _switch_guard = if app_type == crate::app_config::AppType::CodexDesktop.as_str() {
        Some(
            state
                .proxy_service
                .lock_switch_for_app(crate::app_config::AppType::CodexDesktop.as_str())
                .await,
        )
    } else {
        None
    };

    let (auto_failover_enabled, current_provider_id) =
        if app_type == crate::app_config::AppType::CodexDesktop.as_str() {
            let enabled = state
                .db
                .get_codex_desktop_auto_failover_enabled()
                .map_err(|error| error.to_string())?;
            let current = if enabled {
                crate::settings::get_effective_current_provider(
                    &state.db,
                    &crate::app_config::AppType::CodexDesktop,
                )
                .map_err(|error| error.to_string())?
            } else {
                None
            };
            (enabled, current)
        } else {
            (false, None)
        };
    ensure_failover_queue_removal_allowed(
        &app_type,
        &provider_id,
        auto_failover_enabled,
        current_provider_id.as_deref(),
    )?;

    state
        .db
        .remove_from_failover_queue(&app_type, &provider_id)
        .map_err(|e| e.to_string())
}

/// 获取指定应用的自动故障转移开关状态（从 proxy_config 表读取）
#[tauri::command]
pub async fn get_auto_failover_enabled(
    state: tauri::State<'_, AppState>,
    app_type: String,
) -> Result<bool, String> {
    require_failover_app(&app_type)?;
    if app_type == crate::app_config::AppType::CodexDesktop.as_str() {
        return state
            .db
            .get_codex_desktop_auto_failover_enabled()
            .map_err(|error| error.to_string());
    }
    state
        .db
        .get_proxy_config_for_app(&app_type)
        .await
        .map(|config| config.auto_failover_enabled)
        .map_err(|e| e.to_string())
}

/// 设置指定应用的自动故障转移开关状态（写入 proxy_config 表）
///
/// 注意：关闭故障转移时不会清除队列，队列内容会保留供下次开启时使用
#[tauri::command]
pub async fn set_auto_failover_enabled(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    app_type: String,
    enabled: bool,
) -> Result<(), String> {
    require_failover_app(&app_type)?;
    if app_type == crate::app_config::AppType::CodexDesktop.as_str() {
        return set_codex_desktop_auto_failover_enabled(&app, &state, enabled).await;
    }
    log::info!(
        "[Failover] Setting auto_failover_enabled: app_type='{app_type}', enabled={enabled}"
    );

    // 读取当前配置
    let mut config = state
        .db
        .get_proxy_config_for_app(&app_type)
        .await
        .map_err(|e| e.to_string())?;

    if enabled && !config.enabled {
        return Err("需要先启用该应用的代理接管，再开启故障转移".to_string());
    }

    // 队列为空时把当前供应商自动加入作为 P1，避免用户陷入"必须先加队列才能开启"的死锁
    let mut auto_added_provider_id: Option<String> = None;
    let p1_provider_id = if enabled {
        let all_providers = state
            .db
            .get_all_providers(&app_type)
            .map_err(|e| e.to_string())?;
        let mut queue = state
            .db
            .get_failover_queue(&app_type)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|item| {
                all_providers
                    .get(&item.provider_id)
                    .is_some_and(|provider| {
                        crate::proxy::provider_router::provider_supports_failover(
                            &app_type, provider,
                        )
                    })
            })
            .collect::<Vec<_>>();

        if queue.is_empty() {
            let app_enum = crate::app_config::AppType::from_str(&app_type)
                .map_err(|_| format!("无效的应用类型: {app_type}"))?;

            let current_id = crate::settings::get_effective_current_provider(&state.db, &app_enum)
                .map_err(|e| e.to_string())?;

            let Some(current_id) = current_id else {
                return Err("故障转移队列为空，且未设置当前供应商，无法开启故障转移".to_string());
            };

            require_failover_provider(&state.db, &app_type, &current_id)?;

            state
                .db
                .add_to_failover_queue(&app_type, &current_id)
                .map_err(|e| e.to_string())?;
            auto_added_provider_id = Some(current_id);

            queue = state
                .db
                .get_failover_queue(&app_type)
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|item| {
                    all_providers
                        .get(&item.provider_id)
                        .is_some_and(|provider| {
                            crate::proxy::provider_router::provider_supports_failover(
                                &app_type, provider,
                            )
                        })
                })
                .collect();
        }

        queue
            .first()
            .map(|item| item.provider_id.clone())
            .ok_or_else(|| "故障转移队列为空，无法开启故障转移".to_string())?
    } else {
        String::new()
    };

    // 开启前先切到 P1。只有切换成功后才写入 auto_failover_enabled=true，
    // 避免 P1 不可切换（例如 official provider）时留下“开关已开但目标未切”的脏状态。
    if enabled {
        if let Err(e) = state
            .proxy_service
            .switch_proxy_target(&app_type, &p1_provider_id)
            .await
        {
            if let Some(provider_id) = auto_added_provider_id {
                let _ = state.db.remove_from_failover_queue(&app_type, &provider_id);
            }
            return Err(e);
        }
    }

    // 更新 auto_failover_enabled 字段
    config.auto_failover_enabled = enabled;

    // 写回数据库
    state
        .db
        .update_proxy_config_for_app(config)
        .await
        .map_err(|e| e.to_string())?;

    if enabled {
        // 发射 provider-switched 事件（让前端刷新当前供应商）
        let event_data = serde_json::json!({
            "appType": app_type,
            "providerId": p1_provider_id,
            "source": "failoverEnabled"
        });
        let _ = app.emit("provider-switched", event_data);
    }

    // 刷新托盘菜单，确保状态同步
    if let Ok(new_menu) = crate::tray::create_tray_menu(&app, &state) {
        if let Some(tray) = app.tray_by_id(crate::tray::TRAY_ID) {
            let _ = tray.set_menu(Some(new_menu));
        }
    }

    Ok(())
}
