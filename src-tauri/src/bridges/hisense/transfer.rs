use super::{
    applications,
    config::{Action, HisenseConfig, MappingsFile},
    macro_engine::{Definition, Step},
    Runtime,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
use tauri::State;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacroFile {
    pub format: String,
    pub schema_version: u32,
    pub macros: BTreeMap<String, Definition>,
}
#[derive(Serialize)]
pub struct Resolution {
    pub original: String,
    pub target: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
}
#[derive(Serialize)]
pub struct Parsed {
    pub document: serde_json::Value,
    pub config: HisenseConfig,
    pub applications: Vec<Resolution>,
    pub applied: bool,
}

fn relocate(cfg: &mut HisenseConfig) -> Result<Vec<Resolution>, String> {
    let apps = applications::scan(false)?;
    let mut result = BTreeMap::<String, Resolution>::new();
    let mut resolve = |path: &mut String| {
        let original = path.clone();
        match applications::resolve(path, &apps) {
            Ok(target) => {
                *path = target.clone();
                result.insert(
                    original.clone(),
                    Resolution {
                        original,
                        target: Some(target),
                        message: "已关联本机应用".into(),
                        profile_id: None,
                    },
                );
            }
            Err(message) => {
                result.insert(
                    original.clone(),
                    Resolution {
                        original,
                        target: None,
                        message,
                        profile_id: None,
                    },
                );
            }
        }
    };
    for scope in std::iter::once((
        &mut cfg.button_bindings,
        &mut cfg.long_press_bindings,
        &mut cfg.multi_click_bindings,
    ))
    .chain(cfg.profiles.values_mut().map(|p| {
        (
            &mut p.button_bindings,
            &mut p.long_press_bindings,
            &mut p.multi_click_bindings,
        )
    })) {
        for a in scope
            .0
            .values_mut()
            .chain(scope.1.values_mut())
            .chain(scope.2.values_mut().flat_map(|m| m.values_mut()))
        {
            if let Action::LaunchApp(path) = a {
                resolve(path);
            }
        }
    }
    for m in cfg.macros.values_mut() {
        for s in m
            .press
            .iter_mut()
            .chain(m.hold.iter_mut())
            .chain(m.release.iter_mut())
        {
            if let Step::LaunchApp(path) = s {
                resolve(path);
            }
        }
    }
    for (id, p) in &cfg.profiles {
        let found = apps
            .iter()
            .any(|a| a.process_name.eq_ignore_ascii_case(&p.process_name));
        result.entry(format!("配置：{id}")).or_insert(Resolution {
            original: format!("配置：{} · {}", p.name, p.process_name),
            target: found.then(|| p.process_name.clone()),
            profile_id: Some(id.clone()),
            message: if found {
                "已找到对应应用；按前台进程自动切换".into()
            } else {
                "未扫描到应用；保留进程匹配规则，可稍后安装或在应用管理中重新选择".into()
            },
        });
    }
    Ok(result.into_values().collect())
}
pub fn parse(
    text: &str,
    base: &HisenseConfig,
) -> Result<(HisenseConfig, Vec<Resolution>, bool), String> {
    parse_document(text, base, true)
}

fn parse_document(
    text: &str,
    base: &HisenseConfig,
    resolve_applications: bool,
) -> Result<(HisenseConfig, Vec<Resolution>, bool), String> {
    if text.len() > 1_048_576 {
        return Err("映射或宏文件不能超过 1 MB".into());
    }
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| format!("第 {} 行，第 {} 列：{}", e.line(), e.column(), e))?;
    let macros_only = value.get("format").and_then(|s| s.as_str()) == Some("salohub-macros");
    let mut next = base.clone();
    if macros_only {
        let file: MacroFile = serde_json::from_value(value).map_err(|e| e.to_string())?;
        if file.schema_version != 1 {
            return Err("不支持的宏文件版本".into());
        }
        // Merge reusable macros, leaving key bindings and profiles intact.
        next.macros.extend(file.macros);
    } else {
        let file: MappingsFile = serde_json::from_value(value).map_err(|e| e.to_string())?;
        file.apply(&mut next)?;
    }
    next.validate()?;
    // Inventory failure must not discard an otherwise valid document.
    let resolutions = if resolve_applications {
        match relocate(&mut next) {
            Ok(rows) => rows,
            Err(message) => vec![Resolution {
                original: "应用自动关联".into(),
                target: None,
                message,
                profile_id: None,
            }],
        }
    } else {
        Vec::new()
    };
    next.validate()?;
    Ok((next, resolutions, macros_only))
}
#[tauri::command]
pub async fn hisense_parse_mappings(
    text: String,
    apply: bool,
    resolve_applications: Option<bool>,
    runtime: State<'_, Arc<Runtime>>,
) -> Result<Parsed, String> {
    let runtime = runtime.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let base = runtime.config.read().clone();
        let (next, applications, macros_only) =
            parse_document(&text, &base, resolve_applications.unwrap_or(true))?;
        let next = if apply {
            runtime.apply_mappings(MappingsFile::from_config(&next))?
        } else {
            next
        };
        let document = if macros_only {
            serde_json::to_value(MacroFile {
                format: "salohub-macros".into(),
                schema_version: 1,
                macros: next.macros.clone(),
            })
        } else {
            serde_json::to_value(MappingsFile::from_config(&next))
        }
        .map_err(|e| e.to_string())?;
        Ok(Parsed {
            document,
            config: next,
            applications,
            applied: apply,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn hisense_mapping_document(
    runtime: State<'_, Arc<Runtime>>,
    macros_only: bool,
) -> Result<serde_json::Value, String> {
    let cfg = runtime.config.read();
    if macros_only {
        serde_json::to_value(MacroFile {
            format: "salohub-macros".into(),
            schema_version: 1,
            macros: cfg.macros.clone(),
        })
    } else {
        serde_json::to_value(MappingsFile::from_config(&cfg))
    }
    .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn hisense_read_mapping_file(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Read;
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut text = String::new();
        file.take(1_048_577)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        if text.len() > 1_048_576 {
            return Err("文件不能超过 1 MB".into());
        }
        Ok(text)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn hisense_export_document(
    path: String,
    text: String,
    runtime: State<'_, Arc<Runtime>>,
) -> Result<(), String> {
    if text.len() > 1_048_576 {
        return Err("导出文档不能超过 1 MB".into());
    }
    // Syntax/schema validation only. Export never rewrites paths for this PC.
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut next = runtime.config.read().clone();
    if value.get("format").and_then(|s| s.as_str()) == Some("salohub-macros") {
        let doc: MacroFile = serde_json::from_value(value).map_err(|e| e.to_string())?;
        if doc.schema_version != 1 {
            return Err("不支持的宏文件版本".into());
        }
        next.macros.extend(doc.macros);
    } else {
        serde_json::from_value::<MappingsFile>(value)
            .map_err(|e| e.to_string())?
            .apply(&mut next)?;
    }
    next.validate()?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}
