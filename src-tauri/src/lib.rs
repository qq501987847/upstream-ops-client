#![allow(clippy::needless_return)]

use directories::{BaseDirs, ProjectDirs};
use reqwest::{Client, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tauri::State;

const PROFILE_OBJECT: &str = "upstream_ops_client_config";
const PROFILE_VERSION: u32 = 1;

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
    base_url: String,
    api_key: String,
    user_id: Option<i64>,
    access_token: Option<String>,
    state: State<'_, AppState>,
) -> Result<Dashboard, String> {
    let base_url = normalize_base_url(&base_url)?;
    let mut api_key = api_key.trim().to_string();
    let user_id = user_id.unwrap_or_default();
    let access_token = access_token.unwrap_or_default().trim().to_string();
    if api_key.is_empty() && (user_id <= 0 || access_token.is_empty()) {
        return Err("请输入 API Key，或同时填写用户 ID和系统访问令牌".to_string());
    }
    let http = client()?;
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
    )?;
    save_profile_at(&state.profile_path, &profile)?;
    Ok(profile_to_dashboard(&profile, Vec::new(), None))
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

    profile.generated_at = now_marker();
    save_profile_at(&state.profile_path, &profile)?;
    Ok(profile_to_dashboard(&profile, status, status_error))
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
        .expect("error while running New API 客户端");
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
