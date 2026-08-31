# UpstreamOps Client

这是一个 Rust + Tauri 2 桌面客户端，供网关 API Key 持有者查看余额、请求次数、模型和状态，并把当前 Key 可用的模型合并到 WorkBuddy。

## 能力

- 通过公开 Base URL 和 API Key 连接，或导入门户下载的 `upstreamops-client-profile.json`
- 展示剩余/已用积分、累计请求和近 24 小时请求
- 获取当前 Key 可见模型、匿名模型状态、视觉开关、工具调用能力和保守的请求默认值
- 合并写入 WorkBuddy 的 `models.json`，同 ID 模型更新，其他模型和未知字段保留
- 写入现有文件前生成 `models.json.bak-<timestamp>`，Unix 下配置和备份权限设为 `0600`

WorkBuddy 自定义模型目前使用 OpenAI Chat Completions 格式，因此写入的是完整的 `<Base URL>/v1/chat/completions`。网关支持的 Responses、Anthropic Messages 和 Gemini 协议不需要、也不能通过 WorkBuddy 的这份配置声明。

## 服务端前提

管理员需要启用 `gateway.publicPortalEnabled`，并设置绝对的 `gateway.publicBaseURL`。API Key 及其分组必须处于启用状态，分组需要有可见模型。

若要让门户显示客户端下载，设置 `gateway.publicClientDownloadURL` 为本站 `/downloads/upstreamops-client`，并可用 `gateway.publicClientFallbackURL` 配置 GitHub Releases 备用入口。本站安装包使用固定文件名放在服务数据目录的 `client-downloads/` 下。

客户端使用以下接口：

- `GET /v1/portal/client-config`
- `GET /v1/portal/account`
- `GET /v1/portal/models`
- `GET /v1/status`

前三个受保护接口使用原始 API Key；客户端不会把 Key 放进 URL。

## 本地开发

客户端 UI 是静态 HTML/CSS/JavaScript，不需要单独安装 Node 依赖。

```bash
cd src-tauri
cargo tauri dev
```

检查和测试：

```bash
cargo check
cargo test
```

生成当前操作系统安装包：

```bash
# Windows
cargo tauri build --bundles nsis

# macOS
cargo tauri build --bundles dmg

# Linux
cargo tauri build --bundles appimage
```

产物位于 `src-tauri/target/release/bundle/`。Windows 和 macOS 安装包必须分别在 Windows 和 macOS 上构建；Linux 不能直接产出可分发的 NSIS 或 DMG 安装包。

本仓库的 `Build Desktop Client` GitHub Actions 会在版本 tag 上构建 Linux AppImage 和 Windows NSIS 安装包并附加到对应 Release；手动运行时可从 Actions artifacts 下载。

## 合并规则

- 不存在 `models.json` 时创建文件，并让 `availableModels` 只包含本次导入的模型。
- 已有同 ID 模型时更新连接、名称与能力字段，同时保留该模型未识别的自定义字段。
- 已有其他模型和顶层字段保持不变。
- 已有 `availableModels` 为非空数组时追加本次模型；缺失或空数组代表 WorkBuddy 的“显示全部”，保持原语义不变。

## 凭据安全

客户端 profile 和 WorkBuddy `models.json` 都包含明文 API Key，这是 WorkBuddy 配置格式的限制。不要上传、提交或转发这些文件。门户下载响应明确禁止缓存，但下载目录中的文件仍需由用户自行保管。
