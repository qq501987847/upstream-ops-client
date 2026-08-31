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
        // WorkBuddy always calls the validated gateway Base URL.
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
    state: State<'_, AppState>,
) -> Result<Dashboard, String> {
    let base_url = normalize_base_url(&base_url)?;
    let api_key = api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("请输入 API Key".to_string());
    }
    let http = client()?;
    let url = endpoint(&base_url, "/v1/portal/client-config");
    let mut profile: ClientProfile = get_json(&http, &url, Some(&api_key)).await?;
    // Keep the credential the user entered; a remote response cannot replace it.
    profile.api_key = api_key;
    profile.base_url = base_url;
    canonicalize_profile(&mut profile)?;
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

    // client-config is the authoritative snapshot for balance, usage, model
    // capabilities, and defaults. Fetching only /portal/models would lose a
    // newly enabled model's vision flag until the user reconnects manually.
    let base_url = normalize_base_url(&current.base_url)?;
    let api_key = current.api_key.trim().to_string();
    let mut profile: ClientProfile = get_json(
        &http,
        &endpoint(&base_url, "/v1/portal/client-config"),
        Some(&api_key),
    )
    .await?;
    profile.base_url = base_url;
    profile.api_key = api_key;
    canonicalize_profile(&mut profile)?;

    let (status, status_error) =
        match get_json::<StatusResponse>(&http, &endpoint(&profile.base_url, "/v1/status"), None)
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
            import_workbuddy,
            clear_profile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running 火灵连接器");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_profile() -> ClientProfile {
        ClientProfile {
            object: PROFILE_OBJECT.to_string(),
            version: PROFILE_VERSION,
            base_url: "https://gateway.example.com/".to_string(),
            api_key: "sk-test".to_string(),
            models: vec![ClientModel {
                id: "model-a".to_string(),
                name: "model-a".to_string(),
                vendor: "UpstreamOps".to_string(),
                chat_completions_url: "https://gateway.example.com/v1/chat/completions".to_string(),
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
    fn canonicalizes_untrusted_model_urls_to_the_gateway() {
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
            "https://gateway.example.com/v1/chat/completions"
        );

        let directory = test_path("untrusted-url");
        let path = directory.join("models.json");
        import_workbuddy_file(&profile, &path).unwrap();
        let parsed: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            parsed["models"][0]["url"],
            "https://gateway.example.com/v1/chat/completions"
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
            "https://gateway.example.com/v1/chat/completions"
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
            "https://gateway.example.com/v1/chat/completions"
        );
        let _ = fs::remove_dir_all(directory);
    }
}
