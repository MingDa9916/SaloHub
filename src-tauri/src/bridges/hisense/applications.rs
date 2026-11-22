//! Local application catalog and portable executable resolution. Never launches on import.
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Application {
    pub name: String,
    pub process_name: String,
    pub path: String,
    pub source: String,
    #[serde(default)]
    pub launch_id: Option<String>,
    #[serde(default, skip_serializing)]
    logo: Option<String>,
}
static CACHE: Mutex<Option<(Instant, Vec<Application>)>> = parking_lot::const_mutex(None);

pub fn scan(refresh: bool) -> Result<Vec<Application>, String> {
    let mut cache = CACHE.lock();
    if !refresh {
        if let Some((at, items)) = cache.as_ref() {
            if at.elapsed() < Duration::from_secs(300) {
                return Ok(items.clone());
            }
        }
    }
    let mut command = crate::windows_command::background_command("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        include_str!("application_inventory.ps1"),
    ]);
    let output =
        crate::windows_command::software_helper_output(&mut command, Duration::from_secs(60))?;
    if !output.status.success() {
        return Err("应用扫描未完成，请重试或手动选择程序".into());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let items: Vec<Application> = serde_json::from_str(text.trim_start_matches('\u{feff}').trim())
        .map_err(|_| "应用列表解析失败，请刷新重试".to_string())?;
    *cache = Some((Instant::now(), items.clone()));
    Ok(items)
}
pub fn basename(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}
pub fn resolve(path: &str, apps: &[Application]) -> Result<String, String> {
    if path.is_empty() {
        return Err("未指定应用".into());
    }
    if let Some(id) = path.strip_prefix("shell:AppsFolder\\") {
        if apps.iter().any(|a| a.launch_id.as_deref() == Some(id)) {
            return Ok(path.into());
        }
        return Err(format!("未安装商店应用：{id}"));
    }
    if Path::new(path).is_file() {
        return Ok(path.into());
    }
    let name = basename(path);
    let mut matches: Vec<_> = apps
        .iter()
        .filter(|a| a.process_name.eq_ignore_ascii_case(name))
        .map(|a| a.path.clone())
        .collect();
    matches.sort_by_key(|p| p.to_ascii_lowercase());
    matches.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    match matches.as_slice() {
        [p] => Ok(p.clone()),
        [] => Err(format!("未安装或未发现：{name}")),
        _ => Err(format!("存在多个同名程序，请选择：{name}")),
    }
}
pub fn launch(path: &str) -> Result<(), String> {
    if let Some(id) = path.strip_prefix("shell:AppsFolder\\") {
        if !id.is_empty()
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-!".contains(&b))
        {
            return crate::windows_command::background_command("explorer.exe")
                .arg(path)
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string());
        }
        return Err("商店应用标识无效".into());
    }
    if !Path::new(path).is_file() {
        return Err(format!(
            "程序不存在，请在应用列表中重新关联：{}",
            basename(path)
        ));
    }
    crate::windows_command::background_command(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn hisense_scan_applications(refresh: Option<bool>) -> Result<Vec<Application>, String> {
    tauri::async_runtime::spawn_blocking(move || scan(refresh.unwrap_or(false)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn hisense_application_icons(
    paths: Vec<String>,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    if paths.len() > 16 {
        return Err("一次最多读取 16 个应用图标".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let requests: Vec<_> = {
            let cache = CACHE.lock();
            let Some((_, apps)) = cache.as_ref() else {
                return Ok(std::collections::BTreeMap::new());
            };
            paths
                .iter()
                .filter_map(|path| {
                    apps.iter()
                        .find(|app| app.path == *path && app.path.len() <= 512)
                })
                .map(|app| serde_json::json!({ "path": app.path, "logo": app.logo }))
                .collect()
        };
        if requests.is_empty() {
            return Ok(std::collections::BTreeMap::new());
        }
        let mut command = crate::windows_command::background_command("powershell.exe");
        command
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                include_str!("application_icons.ps1"),
            ])
            .env(
                "SALOHUB_ICON_REQUEST",
                serde_json::to_string(&requests).map_err(|e| e.to_string())?,
            );
        let result =
            crate::windows_command::software_helper_output(&mut command, Duration::from_secs(12))?;
        if !result.status.success() {
            return Err("无法读取应用图标".into());
        }
        serde_json::from_slice(&result.stdout).map_err(|_| "无法读取应用图标".into())
    })
    .await
    .map_err(|e| e.to_string())?
}
