use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranslationProvider {
    #[default]
    #[serde(alias = "baidu")]
    BaiduGeneral,
    BaiduLlm,
    Llm,
}

/// 全局设置：settings.json
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppSettings {
    pub hotkey: String,
    #[serde(default)]
    pub launch_at_login: bool,
    #[serde(default)]
    pub translation_provider: TranslationProvider,
    #[serde(default = "default_font")]
    pub ui_body_font_size: f64,
}

/// 百度通用文本翻译：translate-baidu-general.json
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct BaiduGeneralSettings {
    #[serde(default, alias = "baidu_app_id")]
    pub app_id: String,
    #[serde(default, alias = "baidu_app_key")]
    pub app_key: String,
}

/// 百度大模型文本翻译：translate-baidu-llm.json
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BaiduLlmSettings {
    #[serde(default = "default_baidu_llm_api_url")]
    pub api_url: String,
    #[serde(default, alias = "baidu_app_id")]
    pub app_id: String,
    #[serde(default, alias = "baidu_api_key")]
    pub api_key: String,
}

/// 自定义大模型：translate-custom-llm.json
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CustomLlmSettings {
    #[serde(default = "default_llm_api_base")]
    pub api_base: String,
    #[serde(default, alias = "llm_api_key")]
    pub api_key: String,
    #[serde(default = "default_llm_model")]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AllSettings {
    pub general: AppSettings,
    pub baidu_general: BaiduGeneralSettings,
    pub baidu_llm: BaiduLlmSettings,
    pub custom_llm: CustomLlmSettings,
}

fn default_font() -> f64 {
    16.0
}

fn default_llm_api_base() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_baidu_llm_api_url() -> String {
    "https://fanyi-api.baidu.com/ait/api/aiTextTranslate".to_string()
}

fn default_llm_model() -> String {
    "gpt-4o-mini".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: "CommandOrControl+R".to_string(),
            launch_at_login: false,
            translation_provider: TranslationProvider::BaiduGeneral,
            ui_body_font_size: 16.0,
        }
    }
}

impl Default for BaiduLlmSettings {
    fn default() -> Self {
        Self {
            api_url: default_baidu_llm_api_url(),
            app_id: String::new(),
            api_key: String::new(),
        }
    }
}

impl Default for CustomLlmSettings {
    fn default() -> Self {
        Self {
            api_base: default_llm_api_base(),
            api_key: String::new(),
            model: default_llm_model(),
        }
    }
}

impl Default for AllSettings {
    fn default() -> Self {
        Self {
            general: AppSettings::default(),
            baidu_general: BaiduGeneralSettings::default(),
            baidu_llm: BaiduLlmSettings::default(),
            custom_llm: CustomLlmSettings::default(),
        }
    }
}

fn settings_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn load_json<T: for<'de> Deserialize<'de> + Default>(
    path: &PathBuf,
) -> Result<T, String> {
    if !path.exists() {
        return Ok(T::default());
    }
    let data = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

fn save_json<T: Serialize>(path: &PathBuf, value: &T) -> Result<(), String> {
    let data = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, data).map_err(|e| e.to_string())
}

/// 从旧版单文件 settings.json 迁移到分文件存储（仅执行一次）。
fn migrate_legacy_settings(dir: &PathBuf) -> Result<(), String> {
    let legacy = dir.join("settings.json");
    if !legacy.exists() {
        return Ok(());
    }

    let baidu_general_path = dir.join("translate-baidu-general.json");
    let baidu_llm_path = dir.join("translate-baidu-llm.json");
    let custom_llm_path = dir.join("translate-custom-llm.json");

    if baidu_general_path.exists() && baidu_llm_path.exists() && custom_llm_path.exists() {
        return Ok(());
    }

    let data = fs::read_to_string(&legacy).map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&data).unwrap_or(serde_json::Value::Null);
    if !v.is_object() {
        return Ok(());
    }

    if !baidu_general_path.exists() {
        let mut bg = BaiduGeneralSettings::default();
        if let Some(id) = v.get("baidu_app_id").and_then(|x| x.as_str()) {
            bg.app_id = id.to_string();
        }
        if let Some(key) = v.get("baidu_app_key").and_then(|x| x.as_str()) {
            bg.app_key = key.to_string();
        }
        let _ = save_json(&baidu_general_path, &bg);
    }

    if !baidu_llm_path.exists() {
        let mut bl = BaiduLlmSettings::default();
        if let Some(url) = v.get("baidu_api_url").and_then(|x| x.as_str()) {
            bl.api_url = url.to_string();
        }
        if let Some(id) = v.get("baidu_app_id").and_then(|x| x.as_str()) {
            bl.app_id = id.to_string();
        }
        if let Some(key) = v
            .get("baidu_api_key")
            .or_else(|| v.get("baidu_app_key"))
            .and_then(|x| x.as_str())
        {
            bl.api_key = key.to_string();
        }
        let _ = save_json(&baidu_llm_path, &bl);
    }

    if !custom_llm_path.exists() {
        let mut cl = CustomLlmSettings::default();
        if let Some(base) = v.get("llm_api_base").and_then(|x| x.as_str()) {
            cl.api_base = base.to_string();
        }
        if let Some(key) = v.get("llm_api_key").and_then(|x| x.as_str()) {
            cl.api_key = key.to_string();
        }
        if let Some(model) = v.get("llm_model").and_then(|x| x.as_str()) {
            cl.model = model.to_string();
        }
        let _ = save_json(&custom_llm_path, &cl);
    }

    Ok(())
}

pub fn load_all(app: &AppHandle) -> Result<AllSettings, String> {
    let dir = settings_dir(app)?;
    migrate_legacy_settings(&dir)?;

    let general = load_json(&dir.join("settings.json"))?;
    let baidu_general = load_json(&dir.join("translate-baidu-general.json"))?;
    let baidu_llm = load_json(&dir.join("translate-baidu-llm.json"))?;
    let custom_llm = load_json(&dir.join("translate-custom-llm.json"))?;

    Ok(AllSettings {
        general,
        baidu_general,
        baidu_llm,
        custom_llm,
    })
}

pub fn save_all(app: &AppHandle, settings: &AllSettings) -> Result<(), String> {
    let dir = settings_dir(app)?;
    save_json(&dir.join("settings.json"), &settings.general)?;
    save_json(
        &dir.join("translate-baidu-general.json"),
        &settings.baidu_general,
    )?;
    save_json(&dir.join("translate-baidu-llm.json"), &settings.baidu_llm)?;
    save_json(&dir.join("translate-custom-llm.json"), &settings.custom_llm)?;
    Ok(())
}
