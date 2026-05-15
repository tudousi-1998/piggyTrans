use crate::settings::{
    AllSettings, BaiduGeneralSettings, BaiduLlmSettings, CustomLlmSettings, TranslationProvider,
};
use md5::{Digest, Md5};
use serde::Serialize;

const BAIDU_AI_TEXT_TRANSLATE_URL: &str =
    "https://fanyi-api.baidu.com/ait/api/aiTextTranslate";
const BAIDU_VIP_TRANSLATE_URL: &str = "https://fanyi-api.baidu.com/api/trans/vip/translate";

fn normalize_input(text: &str) -> String {
    crate::selection::normalize_line_endings(text)
        .trim()
        .to_string()
}

#[derive(Debug, Serialize)]
pub struct TranslationResult {
    pub original_text: String,
    pub translated_text: String,
    pub detected_source: String,
}

pub async fn translate(all: &AllSettings, text: &str) -> Result<TranslationResult, String> {
    match all.general.translation_provider {
        TranslationProvider::BaiduGeneral => {
            translate_baidu_general(text, &all.baidu_general).await
        }
        TranslationProvider::BaiduLlm => translate_baidu_llm(text, &all.baidu_llm).await,
        TranslationProvider::Llm => translate_custom_llm(text, &all.custom_llm).await,
    }
}

/// 百度通用文本翻译（应用 ID + 密钥，MD5 签名）
pub async fn translate_baidu_general(
    text: &str,
    settings: &BaiduGeneralSettings,
) -> Result<TranslationResult, String> {
    let cleaned = normalize_input(text);
    if cleaned.is_empty() {
        return Err("文本为空".into());
    }
    let app_id = settings.app_id.trim();
    let app_key = settings.app_key.trim();
    if app_id.is_empty() || app_key.is_empty() {
        return Err("请在设置中填写百度通用翻译的「应用 ID」和「密钥」".into());
    }

    let target = resolve_target_language(&cleaned);
    let salt = random_salt();
    let sign_input = format!("{app_id}{cleaned}{salt}{app_key}");
    let sign = format!("{:x}", Md5::digest(sign_input.as_bytes()));

    let client = reqwest::Client::new();
    let url = reqwest::Url::parse_with_params(
        BAIDU_VIP_TRANSLATE_URL,
        &[
            ("q", cleaned.as_str()),
            ("from", "auto"),
            ("to", target),
            ("appid", app_id),
            ("salt", salt.as_str()),
            ("sign", sign.as_str()),
        ],
    )
    .map_err(|e| e.to_string())?;

    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err("翻译服务返回异常".into());
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    if let Some(code) = v.get("error_code") {
        return Err(baidu_error_from_value(&v, code));
    }

    let (src, dst, from) = parse_baidu_translation(&v, &cleaned)?;
    Ok(TranslationResult {
        original_text: src,
        translated_text: dst,
        detected_source: from,
    })
}

/// 百度大模型文本翻译（API 地址 + 应用 ID + API Key）
pub async fn translate_baidu_llm(
    text: &str,
    settings: &BaiduLlmSettings,
) -> Result<TranslationResult, String> {
    let cleaned = normalize_input(text);
    if cleaned.is_empty() {
        return Err("文本为空".into());
    }

    let app_id = settings.app_id.trim();
    let api_key = settings.api_key.trim();
    if app_id.is_empty() {
        return Err("请在设置中填写百度大模型翻译的「应用 ID（appid）」".into());
    }
    if api_key.is_empty() {
        return Err("请在设置中填写百度大模型翻译的「API Key」".into());
    }

    let url = settings.api_url.trim();
    let url = if url.is_empty() {
        BAIDU_AI_TEXT_TRANSLATE_URL
    } else {
        url
    };

    let target = resolve_target_language(&cleaned);
    let body = serde_json::json!({
        "appid": app_id,
        "q": cleaned,
        "from": "auto",
        "to": target,
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求百度大模型翻译失败: {e}"))?;

    let status = resp.status();
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(baidu_http_error(&v, status.as_u16()));
    }

    if let Some(code) = v.get("error_code") {
        return Err(baidu_error_from_value(&v, code));
    }

    let (src, dst, from) = parse_baidu_translation(&v, &cleaned)?;
    Ok(TranslationResult {
        original_text: src,
        translated_text: dst,
        detected_source: from,
    })
}

pub async fn translate_custom_llm(
    text: &str,
    settings: &CustomLlmSettings,
) -> Result<TranslationResult, String> {
    let cleaned = normalize_input(text);
    if cleaned.is_empty() {
        return Err("文本为空".into());
    }

    let api_key = settings.api_key.trim();
    if api_key.is_empty() {
        return Err("请在设置中填写自定义大模型的 API Key".into());
    }

    let model = settings.model.trim();
    if model.is_empty() {
        return Err("请在设置中填写自定义大模型的模型名称".into());
    }

    let target = resolve_target_language(&cleaned);
    let target_label = if target == "zh" { "简体中文" } else { "English" };
    let system_prompt =
        "你是专业翻译助手。只输出译文，不要解释、不要加引号或前后缀。必须保留原文中的换行结构（有几行输入就输出几行译文）。";
    let user_prompt = format!(
        "将以下文本译为{target_label}。规则与百度通用翻译一致：中英混杂译成英文；纯中文译成英文；否则译成中文。保留换行。\n\n{cleaned}"
    );

    let url = chat_completions_url(&settings.api_base);
    let body = serde_json::json!({
        "model": model,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_prompt }
        ]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求大模型失败: {e}"))?;

    let status = resp.status();
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        let msg = v
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .or_else(|| v.get("message").and_then(|m| m.as_str()))
            .unwrap_or("unknown");
        return Err(format!("大模型翻译错误: {msg}"));
    }

    let translated = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "大模型返回内容为空".to_string())?;

    let detected_source = if target == "zh" { "en" } else { "zh" };

    Ok(TranslationResult {
        original_text: cleaned.to_string(),
        translated_text: translated.to_string(),
        detected_source: detected_source.to_string(),
    })
}

fn baidu_error_from_value(v: &serde_json::Value, code: &serde_json::Value) -> String {
    let code_s = code
        .as_str()
        .map(String::from)
        .or_else(|| code.as_i64().map(|n| n.to_string()))
        .unwrap_or_else(|| code.to_string());
    let msg = v
        .get("error_msg")
        .and_then(|m| m.as_str())
        .unwrap_or("unknown");
    format!("百度翻译错误: {code_s} {msg}")
}

fn baidu_http_error(v: &serde_json::Value, status: u16) -> String {
    if let Some(code) = v.get("error_code") {
        return baidu_error_from_value(v, code);
    }
    let msg = v
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("unknown");
    format!("百度翻译 HTTP {status}: {msg}")
}

fn parse_baidu_translation(
    v: &serde_json::Value,
    fallback_src: &str,
) -> Result<(String, String, String), String> {
    if let Some(arr) = v.get("trans_result").and_then(|x| x.as_array()) {
        if arr.is_empty() {
            return Err("翻译服务返回异常".to_string());
        }
        let mut dst_parts: Vec<&str> = Vec::new();
        for item in arr {
            if let Some(d) = item.get("dst").and_then(|s| s.as_str()) {
                dst_parts.push(d);
            }
        }
        if dst_parts.is_empty() {
            return Err("翻译服务返回异常".to_string());
        }
        let dst = dst_parts.join("\n");
        // 原文用用户选中的完整文本，保留换行；API 按行拆分的 src 可能不完整
        let from = v
            .get("from")
            .and_then(|s| s.as_str())
            .unwrap_or("auto")
            .to_string();
        return Ok((fallback_src.to_string(), dst, from));
    }

    if let Some(dst) = v
        .get("result")
        .and_then(|r| r.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let from = v
            .get("from")
            .and_then(|s| s.as_str())
            .unwrap_or("auto")
            .to_string();
        return Ok((fallback_src.to_string(), dst.to_string(), from));
    }

    if let Some(data) = v.get("data") {
        if let Some(dst) = data
            .get("result")
            .and_then(|r| r.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let from = data
                .get("from")
                .or_else(|| v.get("from"))
                .and_then(|s| s.as_str())
                .unwrap_or("auto")
                .to_string();
            return Ok((fallback_src.to_string(), dst.to_string(), from));
        }
    }

    Err("翻译服务返回异常".to_string())
}

fn chat_completions_url(api_base: &str) -> String {
    let base = api_base.trim().trim_end_matches('/');
    if base.is_empty() {
        return "https://api.openai.com/v1/chat/completions".to_string();
    }
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    }
}

/// 含中文（含中英混杂）→ 英文；纯英文等 → 中文。
pub fn resolve_target_language(text: &str) -> &'static str {
    let has_cjk = text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
    if has_cjk {
        "en"
    } else {
        "zh"
    }
}

fn random_salt() -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_micros() % 90_000) as u32 + 10_000)
        .unwrap_or(55_555);
    format!("{n}")
}
