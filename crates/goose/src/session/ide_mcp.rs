//! IDE-managed connections are reconstructed from the authenticated ACP client.
//! Session records retain service identity, never resolved environment/header values.
use crate::config::ExtensionConfig;
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use sqlx::{Pool, Sqlite};

pub(crate) fn is_managed_name(name: &str) -> bool {
    name.starts_with("axiowl_user_")
        || matches!(name, "axiowl_ide" | "axiowl_access" | "axiowl_browser")
}

pub(crate) fn sanitize(value: &mut Value) -> bool {
    let Some(extensions) = value
        .get_mut("enabled_extensions.v0")
        .and_then(|state| state.get_mut("extensions"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    for extension in extensions {
        if !extension
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(is_managed_name)
        {
            continue;
        }
        if let Some(fields) = extension.as_object_mut() {
            for key in ["env", "envs", "headers"] {
                changed |= fields.remove(key).is_some();
            }
        }
    }
    changed
}

pub(crate) fn persisted_json(value: &impl Serialize) -> Result<String> {
    let mut value = serde_json::to_value(value)?;
    sanitize(&mut value);
    Ok(serde_json::to_string(&value)?)
}

pub(crate) fn resolve(
    mut selected: Vec<ExtensionConfig>,
    runtime: &[ExtensionConfig],
) -> Vec<ExtensionConfig> {
    let names: std::collections::HashSet<String> = selected
        .iter()
        .filter(|config| is_managed_name(&config.name()))
        .map(ExtensionConfig::name)
        .collect();
    selected.retain(|config| !is_managed_name(&config.name()));
    selected.extend(
        runtime
            .iter()
            .filter(|config| is_managed_name(&config.name()) && names.contains(&config.name()))
            .cloned(),
    );
    selected
}

pub(crate) async fn repair_legacy(pool: &Pool<Sqlite>) -> Result<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS axiowl_session_repairs (name TEXT PRIMARY KEY)")
        .execute(&mut *tx)
        .await?;
    let done: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM axiowl_session_repairs WHERE name='managed-mcp-secrets-v1')",
    )
    .fetch_one(&mut *tx)
    .await?;
    if done {
        tx.commit().await?;
        return Ok(());
    }
    let mut cursor = String::new();
    loop {
        let rows:Vec<(String,String)>=sqlx::query_as("SELECT id,extension_data FROM sessions WHERE id>? AND extension_data LIKE '%axiowl_%' ORDER BY id LIMIT 128").bind(&cursor).fetch_all(&mut *tx).await?;
        if rows.is_empty() {
            break;
        }
        cursor = rows.last().unwrap().0.clone();
        for (id, text) in rows {
            let mut value: Value = match serde_json::from_str(&text) {
                Ok(value) => value,
                Err(_) => {
                    tracing::warn!(session_id=%id,"Malformed session extension metadata retained during IDE credential cleanup");
                    continue;
                }
            };
            if sanitize(&mut value) {
                sqlx::query("UPDATE sessions SET extension_data=? WHERE id=?")
                    .bind(serde_json::to_string(&value)?)
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    sqlx::query("INSERT INTO axiowl_session_repairs(name) VALUES('managed-mcp-secrets-v1')")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn only_owned_connection_values_are_removed_and_runtime_stays_usable() {
        let original = json!({"enabled_extensions.v0":{"extensions":[
            {"type":"stdio","name":"axiowl_user_fixture","cmd":"fixture","args":[],"envs":{"KEY":"SYNTHETIC_SECRET_SENTINEL"}},
            {"type":"streamable_http","name":"axiowl_access","uri":"http://127.0.0.1/fixture","headers":{"Authorization":"SYNTHETIC_SECRET_SENTINEL"}},
            {"type":"stdio","name":"independent","cmd":"other","args":[],"envs":{"KEEP":"unrelated"}}
        ]},"todo.v0":{"content":"retain user data"}});
        let stored = persisted_json(&original).unwrap();
        assert!(!stored.contains("SYNTHETIC_SECRET_SENTINEL"));
        assert!(stored.contains("unrelated"));
        assert!(stored.contains("retain user data"));
        assert!(original.to_string().contains("SYNTHETIC_SECRET_SENTINEL"));
    }
    #[test]
    fn current_runtime_replaces_stubs_and_omitted_names_stay_removed() {
        fn cfg(name: &str, token: &str) -> ExtensionConfig {
            serde_json::from_value(json!({"type":"stdio","name":name,"cmd":"fixture","args":[],"envs":{"TOKEN":token}})).unwrap()
        }
        let selected = vec![cfg("independent", "kept"), cfg("axiowl_user_current", "")];
        let runtime = vec![
            cfg("axiowl_user_current", "CURRENT_SYNTHETIC"),
            cfg("axiowl_user_removed", "STALE_SYNTHETIC"),
        ];
        let resolved = resolve(selected, &runtime);
        assert_eq!(resolved.len(), 2);
        let text = serde_json::to_string(&resolved).unwrap();
        assert!(text.contains("CURRENT_SYNTHETIC"));
        assert!(!text.contains("STALE_SYNTHETIC"));
        assert_eq!(resolve(vec![cfg("axiowl_user_current", "")], &[]).len(), 0);
    }
    #[tokio::test]
    async fn legacy_cleanup_is_idempotent_and_preserves_unrelated_records() {
        use sqlx::sqlite::SqlitePoolOptions;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE sessions(id TEXT PRIMARY KEY,extension_data TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        let value=json!({"enabled_extensions.v0":{"extensions":[{"name":"axiowl_user_fixture","envs":{"TOKEN":"SYNTHETIC_SECRET_SENTINEL"}},{"name":"independent","envs":{"KEEP":"kept"}}]},"other.v0":{"content":"retained"}}).to_string();
        sqlx::query("INSERT INTO sessions VALUES('fixture',?)")
            .bind(value)
            .execute(&pool)
            .await
            .unwrap();
        repair_legacy(&pool).await.unwrap();
        repair_legacy(&pool).await.unwrap();
        let stored: String = sqlx::query_scalar("SELECT extension_data FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(!stored.contains("SYNTHETIC_SECRET_SENTINEL"));
        assert!(stored.contains("kept"));
        assert!(stored.contains("retained"));
    }
}
