#![allow(clippy::needless_return)]

use directories::{BaseDirs, ProjectDirs};
use reqwest::{Client, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::ErrorKind,
    net::IpAddr,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::State;

const PROFILE_OBJECT: &str = "upstream_ops_client_config";
const PROFILE_VERSION: u32 = 1;
const CLIENT_CONFIG_VERSION: u32 = 1;
const DEFAULT_NOTICE_ROTATION_SECONDS: u64 = 6;
const MIN_NOTICE_ROTATION_SECONDS: u64 = 3;
const MAX_NOTICE_ROTATION_SECONDS: u64 = 30;
const MAX_CLIENT_NOTICES: usize = 3;

/// 客户端内置的默认服务地址：连接器专用域名，与主站 api.fire000.cloud 分离。
/// 地址变更由服务端 client-config 迁移机制接管。
const DEFAULT_BASE_URL: &str = "https://wb.fire000.cloud";

#[derive(Clone)]
struct AppState {
    profile_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct RequestDefaults {
    stream: bool,
    temperature: Option<f64>,
    top_p: Option<f64>,
    max_tokens: Option<i64>,
    reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct Balance {
    limited: bool,
    quota_usd: Option<f64>,
    used_usd: f64,
    remaining_usd: Option<f64>,
    points_per_usd: f64,
    quota_points: Option<f64>,
    used_points: f64,
    remaining_points: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct UsageSummary {
    total_requests: i64,
    last_24h_requests: i64,
    success_count: i64,
    error_count: i64,
    total_input_tokens: i64,
    total_output_tokens: i64,
    total_actual_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct PortalUsage {
    summary: UsageSummary,
    models: Vec<Value>,
    days: Vec<Value>,
    recent: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct StatusModel {
    model: String,
    status: String,
    latency_band: String,
    observed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct StatusResponse {
    object: String,
    updated_at: String,
    base_url: String,
    models: Vec<StatusModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct ApiEndpoints {
    models: String,
    usage: String,
    chat_completions: String,
    responses: String,
    anthropic_messages: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct ClientModel {
    id: String,
    name: String,
    vendor: String,
    chat_completions_url: String,
    supports_tool_call: bool,
    supports_images: bool,
    defaults: RequestDefaults,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct ClientProfile {
    object: String,
    version: u32,
    generated_at: String,
    base_url: String,
    api_key: String,
    user_id: i64,
    access_token: String,
    key_name: String,
    key_prefix: String,
    points_per_usd: f64,
    balance: Balance,
    usage: Option<PortalUsage>,
    models: Vec<ClientModel>,
    available_models: Vec<String>,
    api: ApiEndpoints,
    defaults: RequestDefaults,
    client_config: ClientConfig,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Dashboard {
    connected: bool,
    base_url: String,
    user_id: i64,
    access_token_configured: bool,
    key_name: String,
    key_prefix: String,
    points_per_usd: f64,
    balance: Balance,
    usage: UsageSummary,
    models: Vec<ClientModel>,
    status: Vec<StatusModel>,
    status_error: Option<String>,
    client_config: ClientConfig,
    workbuddy_path: String,
    refreshed_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    path: String,
    backup_path: Option<String>,
    model_count: usize,
    total_model_count: usize,
    available_model_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClientInfo {
    profile_path: String,
    workbuddy_path: String,
    configured: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelTestResult {
    model: String,
    success: bool,
    latency_ms: u128,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RedeemCodeResult {
    quota: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct PortalAccountData {
    object: String,
    key_name: String,
    key_prefix: String,
    #[serde(default)]
    quota: f64,
    #[serde(default)]
    used_quota: f64,
    usage: Option<PortalUsage>,
    models: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct DisplayConfig {
    quota_display_type: String,
    quota_per_unit: f64,
    usd_exchange_rate: f64,
    custom_currency_exchange_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct ClientNotice {
    id: String,
    enabled: bool,
    level: String,
    title: String,
    content: String,
    link_url: String,
    link_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct ClientPromotion {
    enabled: bool,
    title: String,
    description: String,
    button_text: String,
    url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct ClientConfig {
    version: u32,
    base_url: String,
    rotation_interval_seconds: u64,
    notices: Vec<ClientNotice>,
    promotion: ClientPromotion,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            version: CLIENT_CONFIG_VERSION,
            base_url: String::new(),
            rotation_interval_seconds: DEFAULT_NOTICE_ROTATION_SECONDS,
            notices: Vec::new(),
            promotion: ClientPromotion::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
struct PortalClientConfigResponse {
    success: bool,
    data: ClientConfig,
}

impl DisplayConfig {
    fn factor(&self) -> f64 {
        let quota_per_unit = if self.quota_per_unit > 0.0 {
            self.quota_per_unit
        } else {
            500_000.0
        };
        let display_type = self.quota_display_type.trim().to_ascii_uppercase();
        let exchange_rate = match display_type.as_str() {
            "TOKENS" => quota_per_unit,
            "USD" => 1.0,
            "CUSTOM" => {
                if self.custom_currency_exchange_rate > 0.0 {
                    self.custom_currency_exchange_rate
                } else {
                    1.0
                }
            }
            _ => {
                if self.usd_exchange_rate > 0.0 {
                    self.usd_exchange_rate
                } else {
                    1.0
                }
            }
        };
        exchange_rate / quota_per_unit
    }

    fn points_per_usd(&self) -> f64 {
        self.factor()
            * if self.quota_per_unit > 0.0 {
                self.quota_per_unit
            } else {
                500_000.0
            }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct WorkBuddyConfig {
    models: Vec<WorkBuddyModel>,
    #[serde(rename = "availableModels")]
    #[serde(skip_serializing_if = "Option::is_none")]
    available_models: Option<Vec<String>>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct WorkBuddyModel {
    id: String,
    name: String,
    vendor: String,
    #[serde(rename = "apiKey")]
    api_key: String,
    url: String,
    #[serde(rename = "supportsToolCall")]
    supports_tool_call: bool,
    #[serde(rename = "supportsImages")]
    supports_images: bool,
    #[serde(default = "default_stream")]
    stream: bool,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

fn default_stream() -> bool {
    true
}

#[derive(Debug, Clone, Copy)]
enum WorkBuddyFileShape {
    Object,
    Array,
}

fn now_stamp() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    nanos.to_string()
}

fn now_marker() -> String {
    // The static UI converts this dependency-free marker to local date/time.
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default();
    format!("unix:{seconds}")
}

fn profile_path() -> PathBuf {
    ProjectDirs::from("com", "UpstreamOps", "UpstreamOps Client")
        .map(|dirs| dirs.config_dir().join("profile.json"))
        .unwrap_or_else(|| PathBuf::from(".upstreamops-client/profile.json"))
}

fn workbuddy_path() -> PathBuf {
    BaseDirs::new()
        .map(|dirs| dirs.home_dir().join(".workbuddy").join("models.json"))
        .unwrap_or_else(|| PathBuf::from(".workbuddy/models.json"))
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    path.parent()
        .ok_or_else(|| format!("路径没有父目录：{}", path.display()))
        .and_then(|parent| fs::create_dir_all(parent).map_err(|error| error.to_string()))
}

fn set_private_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    ensure_parent(path)?;
    let temp = path.with_file_name(format!(
        ".{}.tmp-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        now_stamp()
    ));
    fs::write(&temp, bytes).map_err(|error| error.to_string())?;
    set_private_permissions(&temp);
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        error.to_string()
    })?;
    set_private_permissions(path);
    Ok(())
}

fn save_profile_at(path: &Path, profile: &ClientProfile) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(profile).map_err(|error| error.to_string())?;
    atomic_write(path, &bytes)
}

fn read_profile_at(path: &Path) -> Result<ClientProfile, String> {
    let bytes = fs::read(path).map_err(|error| match error.kind() {
        ErrorKind::NotFound => "尚未导入客户端配置".to_string(),
        _ => error.to_string(),
    })?;
    let profile: ClientProfile =
        serde_json::from_slice(&bytes).map_err(|error| format!("客户端配置格式无效：{error}"))?;
    validate_profile(&profile)?;
    Ok(profile)
}

fn validate_profile(profile: &ClientProfile) -> Result<(), String> {
    if profile.object != PROFILE_OBJECT || profile.version != PROFILE_VERSION {
        return Err("客户端配置版本不受支持".to_string());
    }
    if profile.api_key.trim().is_empty() {
        return Err("客户端配置缺少 API Key".to_string());
    }
    let _ = normalize_base_url(&profile.base_url)?;
    Ok(())
}

fn canonicalize_profile(profile: &mut ClientProfile) -> Result<(), String> {
    profile.base_url = normalize_base_url(&profile.base_url)?;
    let chat_url = endpoint(&profile.base_url, "/v1/chat/completions");
    let mut seen = BTreeMap::<String, ()>::new();
    profile.models.retain_mut(|model| {
        model.id = model.id.trim().to_string();
        if model.id.is_empty() || seen.insert(model.id.clone(), ()).is_some() {
            return false;
        }
        model.chat_completions_url = chat_url.clone();
        true
    });
    profile.available_models = profile
        .models
        .iter()
        .map(|model| model.id.clone())
        .collect();
    validate_profile(profile)
}

fn normalize_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let url = Url::parse(trimmed).map_err(|error| format!("Base URL 无效：{error}"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("Base URL 必须是绝对 http(s) 地址".to_string());
    }
    if url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Base URL 不能包含账号、密码、查询参数或片段".to_string());
    }
    Ok(trimmed.to_string())
}

/// 连接入口的服务地址解析：为空时回退到内置默认地址，
/// 之后仍可由服务端 client-config 的迁移配置接管。
fn resolve_base_url(raw: &str) -> Result<String, String> {
    if raw.trim().is_empty() {
        return normalize_base_url(DEFAULT_BASE_URL);
    }
    normalize_base_url(raw)
}

fn normalize_external_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    let url = Url::parse(trimmed).map_err(|error| format!("外部链接无效：{error}"))?;
    if url.host_str().is_none() || url.username() != "" || url.password().is_some() {
        return Err("外部链接必须是无账号密码的绝对地址".to_string());
    }
    if url.scheme() == "https" {
        return Ok(trimmed.to_string());
    }
    if url.scheme() != "http" {
        return Err("外部链接必须使用 HTTPS".to_string());
    }
    let host = url.host_str().unwrap_or_default();
    let is_loopback = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false);
    if !is_loopback {
        return Err("非本机外部链接必须使用 HTTPS".to_string());
    }
    Ok(trimmed.to_string())
}

fn normalize_client_config(mut config: ClientConfig) -> Result<ClientConfig, String> {
    if config.version == 0 {
        config.version = CLIENT_CONFIG_VERSION;
    }
    if config.version != CLIENT_CONFIG_VERSION {
        return Err(format!("连接器配置版本 {} 不受支持", config.version));
    }
    if config.rotation_interval_seconds == 0 {
        config.rotation_interval_seconds = DEFAULT_NOTICE_ROTATION_SECONDS;
    }
    if !(MIN_NOTICE_ROTATION_SECONDS..=MAX_NOTICE_ROTATION_SECONDS)
        .contains(&config.rotation_interval_seconds)
    {
        return Err(format!(
            "通知轮播间隔必须在 {MIN_NOTICE_ROTATION_SECONDS} 到 {MAX_NOTICE_ROTATION_SECONDS} 秒之间"
        ));
    }
    if !config.base_url.trim().is_empty() {
        config.base_url = normalize_base_url(&config.base_url)?;
    }

    let mut notices = Vec::with_capacity(MAX_CLIENT_NOTICES);
    for (index, mut notice) in config.notices.into_iter().enumerate() {
        if notices.len() == MAX_CLIENT_NOTICES {
            break;
        }
        notice.id = notice.id.trim().to_string();
        if notice.id.is_empty() {
            notice.id = format!("notice-{}", index + 1);
        }
        notice.level = match notice.level.trim().to_ascii_lowercase().as_str() {
            "warning" => "warning".to_string(),
            "critical" => "critical".to_string(),
            _ => "info".to_string(),
        };
        notice.title = notice.title.trim().to_string();
        notice.content = notice.content.trim().to_string();
        notice.link_text = notice.link_text.trim().to_string();
        notice.link_url = normalize_external_url(&notice.link_url).unwrap_or_default();
        if notice.link_url.is_empty() {
            notice.link_text.clear();
        }
        if notice.enabled && !notice.title.is_empty() {
            notices.push(notice);
        }
    }
    config.notices = notices;

    config.promotion.title = config.promotion.title.trim().to_string();
    config.promotion.description = config.promotion.description.trim().to_string();
    config.promotion.button_text = config.promotion.button_text.trim().to_string();
    config.promotion.url = normalize_external_url(&config.promotion.url).unwrap_or_default();
    if !config.promotion.enabled
        || config.promotion.title.is_empty()
        || config.promotion.button_text.is_empty()
        || config.promotion.url.is_empty()
    {
        config.promotion = ClientPromotion::default();
    }
    Ok(config)
}

fn migration_target(
    current_base_url: &str,
    config: &ClientConfig,
) -> Result<Option<String>, String> {
    if config.base_url.is_empty() {
        return Ok(None);
    }
    let current = Url::parse(current_base_url).map_err(|error| error.to_string())?;
    let target = Url::parse(&config.base_url).map_err(|error| error.to_string())?;
    if current.as_str().trim_end_matches('/') == target.as_str().trim_end_matches('/') {
        return Ok(None);
    }
    if current.scheme() == "https" && target.scheme() != "https" {
        return Err("拒绝把 HTTPS 服务地址自动降级为 HTTP".to_string());
    }
    Ok(Some(config.base_url.clone()))
}

fn endpoint(base_url: &str, path: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), path)
}

fn portal_models(values: &[Value], base_url: &str) -> Vec<ClientModel> {
    let chat_url = endpoint(base_url, "/v1/chat/completions");
    values
        .iter()
        .filter_map(|value| {
            let id = value
                .as_str()
                .or_else(|| value.get("id").and_then(Value::as_str))?
                .trim();
            if id.is_empty() {
                return None;
            }
            Some(ClientModel {
                id: id.to_string(),
                name: value
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(id)
                    .to_string(),
                vendor: value
                    .get("vendor")
                    .and_then(Value::as_str)
                    .unwrap_or("默认服务")
                    .to_string(),
                chat_completions_url: chat_url.clone(),
                supports_tool_call: value
                    .get("supports_tool_call")
                    .or_else(|| value.get("supportsToolCall"))
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                supports_images: value
                    .get("supports_images")
                    .or_else(|| value.get("supportsImages"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                defaults: RequestDefaults::default(),
            })
        })
        .collect()
}

fn profile_from_account(
    base_url: String,
    api_key: String,
    user_id: i64,
    access_token: String,
    payload: Value,
    display_config: &DisplayConfig,
    client_config: ClientConfig,
) -> Result<ClientProfile, String> {
    if payload.get("success").and_then(Value::as_bool) == Some(false) {
        return Err("账户认证失败，请检查服务地址和访问凭据".to_string());
    }
    let data = payload
        .get("data")
        .cloned()
        .ok_or_else(|| "账户响应缺少 data".to_string())?;
    let account: PortalAccountData =
        serde_json::from_value(data).map_err(|error| format!("账户响应格式无效：{error}"))?;
    let user_remaining = account.quota.max(0.0);
    let user_used = account.used_quota.max(0.0);
    let factor = display_config.factor();
    let balance = Balance {
        limited: true,
        // New API's user.quota is already the remaining user balance.
        quota_points: Some(user_remaining * factor),
        used_points: user_used * factor,
        remaining_points: Some(user_remaining * factor),
        ..Default::default()
    };
    let models = portal_models(&account.models, &base_url);
    let mut profile = ClientProfile {
        object: PROFILE_OBJECT.to_string(),
        version: PROFILE_VERSION,
        generated_at: now_marker(),
        base_url,
        api_key,
        user_id,
        access_token,
        key_name: account.key_name,
        key_prefix: account.key_prefix,
        points_per_usd: display_config.points_per_usd(),
        balance,
        usage: account.usage,
        models,
        available_models: Vec::new(),
        api: ApiEndpoints {
            models: "/v1/models".to_string(),
            usage: "/v1/portal/account".to_string(),
            chat_completions: "/v1/chat/completions".to_string(),
            responses: "/v1/responses".to_string(),
            anthropic_messages: "/v1/messages".to_string(),
        },
        defaults: RequestDefaults::default(),
        client_config,
    };
    canonicalize_profile(&mut profile)?;
    Ok(profile)
}

async fn fetch_account(http: &Client, base_url: &str, api_key: &str) -> Result<Value, String> {
    get_json(
        http,
        &endpoint(base_url, "/v1/portal/account"),
        Some(api_key),
    )
    .await
}

async fn fetch_display_config(http: &Client, base_url: &str) -> Result<DisplayConfig, String> {
    let payload: Value = get_json(http, &endpoint(base_url, "/api/status"), None).await?;
    let data = payload.get("data").cloned().unwrap_or(payload);
    serde_json::from_value(data).map_err(|error| format!("展示配置格式无效：{error}"))
}

async fn fetch_client_config(http: &Client, base_url: &str) -> Result<ClientConfig, String> {
    let payload: PortalClientConfigResponse =
        get_json(http, &endpoint(base_url, "/v1/portal/client-config"), None).await?;
    if !payload.success {
        return Err("服务未返回有效的连接器配置".to_string());
    }
    normalize_client_config(payload.data)
}

async fn resolve_client_config(
    http: &Client,
    current_base_url: &str,
    cached_config: &ClientConfig,
) -> (String, ClientConfig, Option<String>) {
    let fallback_config = normalize_client_config(cached_config.clone()).unwrap_or_default();
    let current = match normalize_base_url(current_base_url) {
        Ok(value) => value,
        Err(error) => return (current_base_url.to_string(), fallback_config, Some(error)),
    };
    let config = match fetch_client_config(http, &current).await {
        Ok(value) => value,
        Err(_) => return (current, fallback_config, None),
    };
    let target = match migration_target(&current, &config) {
        Ok(Some(value)) => value,
        Ok(None) => return (current, config, None),
        Err(error) => return (current, config, Some(error)),
    };

    let target_config = match fetch_client_config(http, &target).await {
        Ok(value) => value,
        Err(error) => {
            return (
                current,
                config,
                Some(format!("新服务地址验证失败，仍使用原地址：{error}")),
            )
        }
    };
    if !target_config.base_url.is_empty() && target_config.base_url != target {
        return (
            current,
            config,
            Some("新服务地址返回了不一致的迁移配置，仍使用原地址".to_string()),
        );
    }
    (target, target_config, None)
}

async fn bootstrap_api_key(
    http: &Client,
    base_url: &str,
    user_id: i64,
    access_token: &str,
) -> Result<String, String> {
    let response = http
        .post(endpoint(base_url, "/v1/portal/bootstrap"))
        .header("Accept", "application/json")
        .header("Authorization", access_token)
        .header("New-Api-User", user_id.to_string())
        .send()
        .await
        .map_err(|error| format!("获取 API Key 失败：{error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取 API Key 响应失败：{error}"))?;
    let payload: Value =
        serde_json::from_str(&body).map_err(|error| format!("API Key 响应格式无效：{error}"))?;
    if !status.is_success() || payload.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(format!("获取 API Key 失败（HTTP {}）", status.as_u16()));
    }
    payload
        .get("data")
        .and_then(|data| data.get("api_key"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "获取 API Key 响应缺少 api_key".to_string())
}

fn client() -> Result<Client, String> {
    Client::builder()
        .user_agent("upstreamops-workbuddy/0.1")
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 {
                return attempt.error("重定向次数过多");
            }
            if attempt.previous().iter().any(|url| url.scheme() == "https")
                && attempt.url().scheme() != "https"
            {
                return attempt.error("拒绝把 HTTPS 请求重定向到不安全协议");
            }
            attempt.follow()
        }))
        .build()
        .map_err(|error| format!("初始化网络客户端失败：{error}"))
}

async fn get_json<T: DeserializeOwned>(
    client: &Client,
    url: &str,
    api_key: Option<&str>,
) -> Result<T, String> {
    let mut request = client.get(url).header("Accept", "application/json");
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("请求失败：{error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取响应失败：{error}"))?;
    if !status.is_success() {
        let compact = body.replace(['\n', '\r'], " ");
        let excerpt = compact.chars().take(240).collect::<String>();
        return Err(if excerpt.is_empty() {
            format!("服务返回 HTTP {}", status.as_u16())
        } else {
            format!("服务返回 HTTP {}：{}", status.as_u16(), excerpt)
        });
    }
    serde_json::from_str(&body).map_err(|error| format!("响应格式无效：{error}"))
}

fn profile_to_dashboard(
    profile: &ClientProfile,
    status: Vec<StatusModel>,
    status_error: Option<String>,
) -> Dashboard {
    Dashboard {
        connected: true,
        base_url: profile.base_url.clone(),
        user_id: profile.user_id,
        access_token_configured: !profile.access_token.trim().is_empty(),
        key_name: profile.key_name.clone(),
        key_prefix: profile.key_prefix.clone(),
        points_per_usd: profile.points_per_usd,
        balance: profile.balance.clone(),
        usage: profile.usage.clone().unwrap_or_default().summary,
        models: profile.models.clone(),
        status,
        status_error,
        client_config: profile.client_config.clone(),
        workbuddy_path: workbuddy_path().display().to_string(),
        refreshed_at: now_marker(),
    }
}

fn backup_existing(path: &Path) -> Result<Option<String>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let stamp = now_stamp();
    let backup = path.with_file_name(format!(
        "{}.bak-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        stamp
    ));
    fs::copy(path, &backup).map_err(|error| error.to_string())?;
    set_private_permissions(&backup);
    Ok(Some(backup.display().to_string()))
}

fn workbuddy_model(profile: &ClientProfile, model: &ClientModel) -> WorkBuddyModel {
    WorkBuddyModel {
        id: model.id.clone(),
        name: if model.name.trim().is_empty() {
            model.id.clone()
        } else {
            model.name.clone()
        },
        vendor: if model.vendor.trim().is_empty() {
            "UpstreamOps".to_string()
        } else {
            model.vendor.clone()
        },
        api_key: profile.api_key.clone(),
        // Never trust a profile-supplied per-model URL with a live API key.
        // WorkBuddy always calls the validated New API Base URL.
        url: endpoint(&profile.base_url, "/v1/chat/completions"),
        supports_tool_call: model.supports_tool_call,
        supports_images: model.supports_images,
        // WorkBuddy should use streaming by default for every imported model.
        stream: true,
        extra: BTreeMap::new(),
    }
}

fn import_workbuddy_file(profile: &ClientProfile, path: &Path) -> Result<ImportResult, String> {
    let existed = path.is_file();
    if !existed && profile.models.is_empty() {
        return Err("当前分组没有可导入的模型".to_string());
    }
    let (mut config, shape) = if existed {
        let bytes = fs::read(path).map_err(|error| error.to_string())?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("WorkBuddy 配置格式无效：{error}"))?;
        if value.is_array() {
            let models = serde_json::from_value::<Vec<WorkBuddyModel>>(value)
                .map_err(|error| format!("WorkBuddy 模型配置格式无效：{error}"))?;
            (
                WorkBuddyConfig {
                    models,
                    ..Default::default()
                },
                WorkBuddyFileShape::Array,
            )
        } else {
            let config = serde_json::from_value::<WorkBuddyConfig>(value)
                .map_err(|error| format!("WorkBuddy 配置格式无效：{error}"))?;
            (config, WorkBuddyFileShape::Object)
        }
    } else {
        (WorkBuddyConfig::default(), WorkBuddyFileShape::Object)
    };

    let incoming = profile
        .models
        .iter()
        .map(|model| workbuddy_model(profile, model))
        .collect::<Vec<_>>();
    for model in incoming {
        if let Some(existing) = config.models.iter_mut().find(|item| item.id == model.id) {
            let mut replacement = model;
            replacement.extra = std::mem::take(&mut existing.extra);
            *existing = replacement;
        } else {
            config.models.push(model);
        }
    }

    if existed && matches!(shape, WorkBuddyFileShape::Object) {
        // Missing or empty availableModels means "show all" in WorkBuddy. Keep
        // that meaning instead of turning it into a restrictive filter.
        if let Some(ids) = config
            .available_models
            .as_mut()
            .filter(|ids| !ids.is_empty())
        {
            for id in &profile.available_models {
                if !id.trim().is_empty() && !ids.iter().any(|item| item == id) {
                    ids.push(id.clone());
                }
            }
        }
    } else {
        config.available_models = Some(profile.available_models.clone());
    }

    let backup_path = backup_existing(path)?;
    let output = if matches!(shape, WorkBuddyFileShape::Array) {
        serde_json::to_value(&config.models).map_err(|error| error.to_string())?
    } else {
        serde_json::to_value(&config).map_err(|error| error.to_string())?
    };
    let bytes = serde_json::to_vec_pretty(&output).map_err(|error| error.to_string())?;
    atomic_write(path, &bytes)?;
    Ok(ImportResult {
        path: path.display().to_string(),
        backup_path,
        model_count: profile.models.len(),
        total_model_count: config.models.len(),
        available_model_count: config
            .available_models
            .as_ref()
            .map(|items| items.len())
            .unwrap_or_default(),
    })
}

#[tauri::command]
async fn connect(
    base_url: Option<String>,
    api_key: String,
    user_id: Option<i64>,
    access_token: Option<String>,
    state: State<'_, AppState>,
) -> Result<Dashboard, String> {
    let base_url = resolve_base_url(base_url.as_deref().unwrap_or(""))?;
    let mut api_key = api_key.trim().to_string();
    let user_id = user_id.unwrap_or_default();
    let access_token = access_token.unwrap_or_default().trim().to_string();
    if api_key.is_empty() && (user_id <= 0 || access_token.is_empty()) {
        return Err("请输入 API Key，或同时填写用户 ID和系统访问令牌".to_string());
    }
    let http = client()?;
    let (base_url, client_config, config_error) =
        resolve_client_config(&http, &base_url, &ClientConfig::default()).await;
    if api_key.is_empty() {
        api_key = bootstrap_api_key(&http, &base_url, user_id, &access_token).await?;
    }
    if api_key.is_empty() {
        return Err("请输入 API Key".to_string());
    }
    let display_config = fetch_display_config(&http, &base_url)
        .await
        .unwrap_or_default();
    let payload = fetch_account(&http, &base_url, &api_key).await?;
    let (stored_user_id, stored_access_token) = read_profile_at(&state.profile_path)
        .ok()
        .filter(|current| current.api_key == api_key && current.base_url == base_url)
        .map(|current| (current.user_id, current.access_token))
        .unwrap_or_default();
    let profile = profile_from_account(
        base_url,
        api_key,
        if user_id > 0 { user_id } else { stored_user_id },
        if access_token.is_empty() {
            stored_access_token
        } else {
            access_token
        },
        payload,
        &display_config,
        client_config,
    )?;
    save_profile_at(&state.profile_path, &profile)?;
    Ok(profile_to_dashboard(&profile, Vec::new(), config_error))
}

#[tauri::command]
fn import_profile(profile: String, state: State<'_, AppState>) -> Result<Dashboard, String> {
    let mut parsed: ClientProfile =
        serde_json::from_str(&profile).map_err(|error| format!("客户端配置格式无效：{error}"))?;
    canonicalize_profile(&mut parsed)?;
    save_profile_at(&state.profile_path, &parsed)?;
    Ok(profile_to_dashboard(&parsed, Vec::new(), None))
}

#[tauri::command]
fn load_dashboard(state: State<'_, AppState>) -> Result<Dashboard, String> {
    let profile = read_profile_at(&state.profile_path)?;
    Ok(profile_to_dashboard(&profile, Vec::new(), None))
}

#[tauri::command]
fn client_info(state: State<'_, AppState>) -> ClientInfo {
    ClientInfo {
        profile_path: state.profile_path.display().to_string(),
        workbuddy_path: workbuddy_path().display().to_string(),
        configured: state.profile_path.is_file(),
    }
}

#[tauri::command]
async fn refresh_dashboard(state: State<'_, AppState>) -> Result<Dashboard, String> {
    let current = read_profile_at(&state.profile_path)?;
    let http = client()?;

    let base_url = normalize_base_url(&current.base_url)?;
    let (base_url, client_config, config_error) =
        resolve_client_config(&http, &base_url, &current.client_config).await;
    let api_key = current.api_key.trim().to_string();
    let display_config = fetch_display_config(&http, &base_url)
        .await
        .unwrap_or_default();
    let payload = fetch_account(&http, &base_url, &api_key).await?;
    let mut profile = profile_from_account(
        base_url,
        api_key,
        current.user_id,
        current.access_token,
        payload,
        &display_config,
        client_config,
    )?;

    let (status, status_error) = match get_json::<StatusResponse>(
        &http,
        &endpoint(&profile.base_url, "/v1/portal/status"),
        None,
    )
    .await
    {
        Ok(payload) => (payload.models, None),
        Err(error) => (Vec::new(), Some(error)),
    };

    let status_error = [config_error, status_error]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("；");
    profile.generated_at = now_marker();
    save_profile_at(&state.profile_path, &profile)?;
    Ok(profile_to_dashboard(
        &profile,
        status,
        if status_error.is_empty() {
            None
        } else {
            Some(status_error)
        },
    ))
}

#[tauri::command]
async fn test_model(model: String, state: State<'_, AppState>) -> Result<ModelTestResult, String> {
    let profile = read_profile_at(&state.profile_path)?;
    let model_id = model.trim();
    if model_id.is_empty() {
        return Err("模型名称不能为空".to_string());
    }
    if !profile.models.iter().any(|item| item.id == model_id) {
        return Err("该模型不在当前 API Key 的可用列表中".to_string());
    }

    let http = client()?;
    let payload = serde_json::json!({
        "model": model_id,
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 1,
        "stream": false,
    });
    let started = Instant::now();
    let response = http
        .post(endpoint(&profile.base_url, "/v1/chat/completions"))
        .bearer_auth(profile.api_key.trim())
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|error| format!("请求失败：{error}"))?;
    let latency_ms = started.elapsed().as_millis();
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取响应失败：{error}"))?;
    if status.is_success() {
        Ok(ModelTestResult {
            model: model_id.to_string(),
            success: true,
            latency_ms,
            detail: format!("连接成功，HTTP {}", status.as_u16()),
        })
    } else {
        let excerpt = body
            .replace(['\n', '\r'], " ")
            .chars()
            .take(240)
            .collect::<String>();
        Ok(ModelTestResult {
            model: model_id.to_string(),
            success: false,
            latency_ms,
            detail: if excerpt.is_empty() {
                format!("上游返回 HTTP {}", status.as_u16())
            } else {
                format!("上游返回 HTTP {}：{}", status.as_u16(), excerpt)
            },
        })
    }
}

#[tauri::command]
async fn redeem_code(code: String, state: State<'_, AppState>) -> Result<RedeemCodeResult, String> {
    let code = code.trim();
    if code.is_empty() {
        return Err("请输入兑换码".to_string());
    }
    let profile = read_profile_at(&state.profile_path)?;
    if profile.access_token.trim().is_empty() {
        return Err("当前配置没有系统访问令牌".to_string());
    }
    let http = client()?;
    let response = http
        .post(endpoint(&profile.base_url, "/api/user/topup"))
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .bearer_auth(profile.access_token.trim())
        .json(&serde_json::json!({ "key": code }))
        .send()
        .await
        .map_err(|error| format!("兑换请求失败：{error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("读取兑换响应失败：{error}"))?;
    let payload: Value =
        serde_json::from_str(&body).map_err(|error| format!("兑换响应格式无效：{error}"))?;
    if !status.is_success() || payload.get("success").and_then(Value::as_bool) == Some(false) {
        let excerpt = body
            .replace(['\n', '\r'], " ")
            .chars()
            .take(240)
            .collect::<String>();
        return Err(if excerpt.is_empty() {
            format!("服务返回 HTTP {}", status.as_u16())
        } else {
            format!("服务返回 HTTP {}：{}", status.as_u16(), excerpt)
        });
    }
    let quota = payload
        .get("data")
        .and_then(Value::as_i64)
        .ok_or_else(|| "兑换响应缺少额度".to_string())?;
    Ok(RedeemCodeResult { quota })
}

#[tauri::command]
fn import_workbuddy(state: State<'_, AppState>) -> Result<ImportResult, String> {
    let profile = read_profile_at(&state.profile_path)?;
    import_workbuddy_file(&profile, &workbuddy_path())
}

#[tauri::command]
fn clear_profile(state: State<'_, AppState>) -> Result<(), String> {
    match fs::remove_file(&state.profile_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

pub fn run() {
    let state = AppState {
        profile_path: profile_path(),
    };
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            connect,
            import_profile,
            load_dashboard,
            client_info,
            refresh_dashboard,
            test_model,
            redeem_code,
            import_workbuddy,
            clear_profile,
        ])
        .run(tauri::generate_context!())
        .expect("火灵连接器启动失败");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_profile() -> ClientProfile {
        ClientProfile {
            object: PROFILE_OBJECT.to_string(),
            version: PROFILE_VERSION,
            base_url: "https://new-api.example.com/".to_string(),
            api_key: "sk-test".to_string(),
            models: vec![ClientModel {
                id: "model-a".to_string(),
                name: "model-a".to_string(),
                vendor: "UpstreamOps".to_string(),
                chat_completions_url: "https://new-api.example.com/v1/chat/completions".to_string(),
                supports_tool_call: true,
                supports_images: true,
                defaults: RequestDefaults {
                    stream: false,
                    ..Default::default()
                },
            }],
            available_models: vec!["model-a".to_string()],
            ..Default::default()
        }
    }

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "uops-workbuddy-{name}-{}-{}",
            std::process::id(),
            now_stamp()
        ))
    }

    #[test]
    fn rejects_unsafe_base_url() {
        assert!(normalize_base_url("ftp://example.com").is_err());
        assert!(normalize_base_url("https://user:pass@example.com").is_err());
        assert_eq!(
            normalize_base_url("https://example.com///").unwrap(),
            "https://example.com"
        );
    }

    #[test]
    fn connect_base_url_falls_back_to_builtin_default() {
        assert_eq!(resolve_base_url("").unwrap(), DEFAULT_BASE_URL);
        assert_eq!(resolve_base_url("   ").unwrap(), DEFAULT_BASE_URL);
        assert_eq!(
            resolve_base_url("http://127.0.0.1:3000/").unwrap(),
            "http://127.0.0.1:3000"
        );
        assert!(resolve_base_url("not a url").is_err());
    }

    #[test]
    fn dashboard_balance_uses_user_quota_not_token_quota() {
        let payload = serde_json::json!({
            "success": true,
            "data": {
                "key_name": "Portal API Key",
                "key_prefix": "sk-abcd",
                "quota": 1_000_000,
                "used_quota": 125_000,
                "remain_quota": 10_000,
                "used_token_quota": 2_000,
                "models": []
            }
        });
        let display_config = DisplayConfig {
            quota_display_type: "CUSTOM".to_string(),
            quota_per_unit: 500_000.0,
            custom_currency_exchange_rate: 600.0,
            ..Default::default()
        };
        let profile = profile_from_account(
            "https://example.com".to_string(),
            "sk-test".to_string(),
            42,
            "system-token".to_string(),
            payload,
            &display_config,
            ClientConfig::default(),
        )
        .unwrap();
        assert_eq!(profile.balance.quota_points, Some(1200.0));
        assert_eq!(profile.balance.used_points, 150.0);
        assert_eq!(profile.balance.remaining_points, Some(1200.0));
    }

    #[test]
    fn canonicalizes_untrusted_model_urls_to_new_api() {
        let mut profile = sample_profile();
        profile.models[0].chat_completions_url = "https://attacker.example/collect-key".to_string();
        profile.models.push(profile.models[0].clone());
        profile.models.push(ClientModel {
            id: "  ".to_string(),
            ..Default::default()
        });
        canonicalize_profile(&mut profile).unwrap();
        assert_eq!(profile.models.len(), 1);
        assert_eq!(profile.available_models, vec!["model-a"]);
        assert_eq!(
            profile.models[0].chat_completions_url,
            "https://new-api.example.com/v1/chat/completions"
        );

        let directory = test_path("untrusted-url");
        let path = directory.join("models.json");
        import_workbuddy_file(&profile, &path).unwrap();
        let parsed: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            parsed["models"][0]["url"],
            "https://new-api.example.com/v1/chat/completions"
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn connector_config_keeps_only_three_enabled_valid_notices() {
        let config = normalize_client_config(ClientConfig {
            notices: vec![
                ClientNotice {
                    enabled: false,
                    title: "Draft".to_string(),
                    ..Default::default()
                },
                ClientNotice {
                    enabled: true,
                    level: "warning".to_string(),
                    title: " One ".to_string(),
                    ..Default::default()
                },
                ClientNotice {
                    enabled: true,
                    level: "unknown".to_string(),
                    title: "Two".to_string(),
                    link_url: "javascript:alert(1)".to_string(),
                    link_text: "Unsafe".to_string(),
                    ..Default::default()
                },
                ClientNotice {
                    enabled: true,
                    title: "Three".to_string(),
                    ..Default::default()
                },
                ClientNotice {
                    enabled: true,
                    title: "Four".to_string(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        })
        .unwrap();

        assert_eq!(config.notices.len(), 3);
        assert_eq!(config.notices[0].title, "One");
        assert_eq!(config.notices[1].level, "info");
        assert!(config.notices[1].link_url.is_empty());
        assert!(config.notices[1].link_text.is_empty());
        assert_eq!(config.notices[2].title, "Three");
    }

    #[test]
    fn connector_migration_never_downgrades_https() {
        let insecure = ClientConfig {
            base_url: "http://api.example.com".to_string(),
            ..Default::default()
        };
        assert!(migration_target("https://old.example.com", &insecure).is_err());

        let secure = ClientConfig {
            base_url: "https://api.example.com".to_string(),
            ..Default::default()
        };
        assert_eq!(
            migration_target("https://old.example.com", &secure).unwrap(),
            Some("https://api.example.com".to_string())
        );
    }

    #[test]
    fn merges_workbuddy_models_and_preserves_unknown_fields() {
        let directory = test_path("merge");
        let path = directory.join("models.json");
        fs::create_dir_all(&directory).unwrap();
        fs::write(&path, br#"{"models":[{"id":"model-a","name":"Old","vendor":"Other","apiKey":"old","url":"https://old","custom":true},{"id":"old","name":"Old","vendor":"Other","apiKey":"old","url":"https://old"}],"availableModels":["old"],"other":42}"#).unwrap();
        let result = import_workbuddy_file(&sample_profile(), &path).unwrap();
        assert_eq!(result.model_count, 1);
        assert_eq!(result.total_model_count, 2);
        let parsed: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(parsed["other"], 42);
        assert_eq!(parsed["models"][0]["apiKey"], "sk-test");
        assert_eq!(parsed["models"][0]["custom"], true);
        assert_eq!(parsed["models"][1]["id"], "old");
        assert_eq!(
            parsed["availableModels"],
            serde_json::json!(["old", "model-a"])
        );
        assert!(result.backup_path.is_some());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn preserves_show_all_available_models_semantics() {
        for existing in [r#"{"models":[]}"#, r#"{"models":[],"availableModels":[]}"#] {
            let directory = test_path("available");
            let path = directory.join("models.json");
            fs::create_dir_all(&directory).unwrap();
            fs::write(&path, existing).unwrap();
            import_workbuddy_file(&sample_profile(), &path).unwrap();
            let parsed: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            if existing.contains("availableModels") {
                assert_eq!(parsed["availableModels"], serde_json::json!([]));
            } else {
                assert!(parsed.get("availableModels").is_none());
            }
            let _ = fs::remove_dir_all(directory);
        }
    }

    #[test]
    fn imports_top_level_workbuddy_model_arrays() {
        let directory = test_path("array");
        let path = directory.join("models.json");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            &path,
            br#"[{"id":"old","name":"Old","vendor":"Other","apiKey":"old","url":"https://old"}]"#,
        )
        .unwrap();

        let result = import_workbuddy_file(&sample_profile(), &path).unwrap();
        assert_eq!(result.total_model_count, 2);
        let parsed: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed[1]["id"], "model-a");
        assert_eq!(
            parsed[1]["url"],
            "https://new-api.example.com/v1/chat/completions"
        );
        assert_eq!(parsed[1]["stream"], true);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn creates_a_restrictive_list_for_a_new_workbuddy_file() {
        let directory = test_path("new");
        let path = directory.join("models.json");
        let result = import_workbuddy_file(&sample_profile(), &path).unwrap();
        assert_eq!(result.model_count, 1);
        let parsed: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(parsed["availableModels"], serde_json::json!(["model-a"]));
        assert_eq!(
            parsed["models"][0]["url"],
            "https://new-api.example.com/v1/chat/completions"
        );
        let _ = fs::remove_dir_all(directory);
    }
}
