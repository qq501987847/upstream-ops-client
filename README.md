# 火灵连接器

这是一个 Rust + Tauri 2 桌面客户端，连接真实的 New API 服务，查看账户额度、请求次数和可用模型，并把模型配置合并到 WorkBuddy。

## 能力

- 使用内置服务地址连接预创建账户，用户只需填写用户 ID 和系统访问令牌；地址后续由服务端 client-config 迁移机制统一管理
- 展示剩余/已用额度、累计请求和近 24 小时请求
- 获取当前账户可用模型并支持逐模型实际连通测试
- 提供“主页”和“充值与兑换”菜单
- 从管理后台读取最多 3 条顶部通知，按配置间隔轮播，悬停或聚焦时暂停
- 充值页可展示管理员配置的官方小铺或活动入口；未启用时整块隐藏
- 支持服务地址安全迁移：先验证新地址的连接器配置接口，且禁止 HTTPS 自动降级为 HTTP
- 使用预创建账户的用户 ID 和系统访问令牌自动领取 API Key
- 云猫购买普通兑换码后，通过系统令牌调用 New API 普通充值接口增加账户额度
- 合并写入 WorkBuddy 的 `models.json`，同 ID 模型更新，其他模型和未知字段保留
- 写入现有文件前生成 `models.json.bak-<timestamp>`，Unix 下配置和备份权限设为 `0600`

WorkBuddy 使用 OpenAI Chat Completions 格式，因此写入的是完整的 `<New API 地址>/v1/chat/completions`。

## 服务端接口

客户端使用 New API 原生门户接口：

- `GET /v1/portal/account`：读取账户、额度和模型
- `POST /v1/portal/bootstrap`：使用预创建账户的用户 ID 和系统访问令牌领取 Portal API Key
- `GET /v1/portal/status`：读取服务状态
- `GET /v1/portal/client-config`：读取通知、充值页运营位和服务地址迁移配置
- `POST /api/user/topup`：使用系统访问令牌兑换普通兑换码并增加用户额度
- `POST /v1/chat/completions`：模型连通测试

续费请求的认证头为 `Authorization: <access_token>` 和 `New-Api-User: <user_id>`。系统访问令牌是敏感凭据，客户端只写入本地 profile，不显示完整内容，也不会写入 URL 或日志。

## 本地开发

客户端 UI 是静态 HTML/CSS/JavaScript，不需要单独安装 Node 依赖。

### 浏览器开发模式

没有后端或凭据时，可使用仅存在于浏览器内存中的 mock 数据开发 UI。该模式不会调用网络，也不会写入 profile 或 WorkBuddy 配置：

```bash
cd ui
python3 -m http.server 4173
```

然后打开 <http://127.0.0.1:4173/?dev=1>。页面右上角出现“开发模式”标记即表示已启用。

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

产物位于 `src-tauri/target/release/bundle/`。

## WorkBuddy 合并规则

- 不存在 `models.json` 时创建文件，并让 `availableModels` 只包含本次导入的模型。
- 已有同 ID 模型时更新连接、名称与能力字段，同时保留该模型未识别的自定义字段。
- 已有其他模型和顶层字段保持不变。
- 已有 `availableModels` 为非空数组时追加本次模型；缺失或空数组代表 WorkBuddy 的“显示全部”，保持原语义不变。
- 如果现有 `models.json` 是顶层模型数组，则按数组格式原地合并。
- 写入的每个模型统一使用当前 New API 地址和 API Key，并默认启用 `stream=true`。

## 凭据安全

客户端 profile 和 WorkBuddy `models.json` 都包含明文 API Key；profile 还包含系统访问令牌。这些文件仅用于本机运行，权限已限制为用户可读写。不要上传、提交或转发它们。
