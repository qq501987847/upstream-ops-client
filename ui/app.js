const tauriInvoke = window.__TAURI__?.core?.invoke
const query = new URLSearchParams(window.location.search)
// Browser development mode is intentionally opt-in and never active in Tauri.
const developmentMode = !tauriInvoke && query.get("dev") === "1"

const developmentState = {
  dashboard: null,
}

function developmentDashboard(baseUrl = "http://127.0.0.1:3000", apiKey = "sk-dev") {
  const models = [
    { id: "dev-chat", name: "开发对话模型", vendor: "开发环境", supportsToolCall: true, supportsImages: false },
    { id: "dev-vision", name: "开发视觉模型", vendor: "开发环境", supportsToolCall: true, supportsImages: true },
  ]
  return {
    connected: true,
    baseUrl,
    keyName: "开发模式 API Key",
    keyPrefix: `${apiKey.slice(0, 8)}…`,
    userId: 10001,
    accessTokenConfigured: true,
    pointsPerUsd: 1,
    balance: { limited: true, remaining_points: 1280, used_points: 320 },
    usage: { total_requests: 42, last_24h_requests: 7 },
    models,
    status: models.map((model) => ({ model: model.id, status: "available", latency_band: "fast" })),
    statusError: null,
    workbuddyPath: "开发模式不会写入本机 WorkBuddy 配置",
    refreshedAt: `unix:${Math.floor(Date.now() / 1000)}`,
  }
}

function developmentInvoke(command, args = {}) {
  switch (command) {
    case "client_info":
      return { profilePath: "开发模式内存配置", workbuddyPath: "开发模式不会写入本机配置", configured: Boolean(developmentState.dashboard) }
    case "load_dashboard":
      if (!developmentState.dashboard) throw new Error("开发模式尚未连接")
      return developmentState.dashboard
    case "connect":
      developmentState.dashboard = developmentDashboard(args.baseUrl, args.apiKey)
      return developmentState.dashboard
    case "refresh_dashboard":
      if (!developmentState.dashboard) throw new Error("开发模式尚未连接")
      developmentState.dashboard = { ...developmentState.dashboard, refreshedAt: `unix:${Math.floor(Date.now() / 1000)}` }
      return developmentState.dashboard
    case "test_model":
      return { model: args.model, success: true, latencyMs: 18, detail: "开发模式模拟成功" }
    case "redeem_code":
      if (!args.code?.trim()) throw new Error("请输入兑换码")
      if (developmentState.dashboard) {
        developmentState.dashboard = {
          ...developmentState.dashboard,
          balance: {
            ...developmentState.dashboard.balance,
            remaining_points: (developmentState.dashboard.balance?.remaining_points ?? 0) + 100000,
          },
        }
      }
      return { quota: 100000 }
    case "import_workbuddy":
      return { path: "开发模式不会写入本机 WorkBuddy 配置", backupPath: null, modelCount: developmentState.dashboard?.models.length ?? 0, totalModelCount: developmentState.dashboard?.models.length ?? 0, availableModelCount: developmentState.dashboard?.models.length ?? 0 }
    case "clear_profile":
      developmentState.dashboard = null
      return null
    default:
      throw new Error(`开发模式不支持命令：${command}`)
  }
}

const elements = {
  homeView: document.querySelector("#home-view"),
  setupView: document.querySelector("#setup-view"),
  dashboardView: document.querySelector("#dashboard-view"),
  rechargeView: document.querySelector("#recharge-view"),
  navItems: [...document.querySelectorAll("[data-view]")],
  connectForm: document.querySelector("#connect-form"),
  connectButton: document.querySelector("#connect-button"),
  baseURL: document.querySelector("#base-url"),
  apiKey: document.querySelector("#api-key"),
  userId: document.querySelector("#user-id"),
  accessToken: document.querySelector("#access-token"),
  setupDescription: document.querySelector("#setup-description"),
  connectionLabel: document.querySelector("#connection-label"),
  refreshButton: document.querySelector("#refresh-button"),
  disconnectButton: document.querySelector("#disconnect-button"),
  dashboardKey: document.querySelector("#dashboard-key"),
  dashboardUser: document.querySelector("#dashboard-user"),
  dashboardBaseURL: document.querySelector("#dashboard-base-url"),
  refreshTime: document.querySelector("#refresh-time"),
  remainingPoints: document.querySelector("#remaining-points"),
  usedPoints: document.querySelector("#used-points"),
  totalRequests: document.querySelector("#total-requests"),
  last24Requests: document.querySelector("#last24-requests"),
  modelCount: document.querySelector("#model-count"),
  modelWarning: document.querySelector("#model-warning"),
  modelRows: document.querySelector("#model-rows"),
  workbuddyPath: document.querySelector("#workbuddy-path"),
  workbuddyButton: document.querySelector("#workbuddy-button"),
  workbuddyResult: document.querySelector("#workbuddy-result"),
  message: document.querySelector("#message"),
  developmentBadge: document.querySelector("#development-badge"),
  redeemForm: document.querySelector("#redeem-form"),
  redeemCode: document.querySelector("#redeem-code"),
  redeemButton: document.querySelector("#redeem-button"),
  redeemResult: document.querySelector("#redeem-result"),
  purchaseButton: document.querySelector("#purchase-button"),
  purchaseResult: document.querySelector("#purchase-result"),
}

let messageTimer

function invoke(command, args) {
  if (!tauriInvoke) {
    if (developmentMode) return Promise.resolve().then(() => developmentInvoke(command, args))
    return Promise.reject(new Error("请通过火灵 API 客户端桌面程序打开此页面"))
  }
  return tauriInvoke(command, args)
}

function setBusy(button, busy, busyText, idleText) {
  button.disabled = busy
  button.textContent = busy ? busyText : idleText
}

function showMessage(text, error = false) {
  window.clearTimeout(messageTimer)
  elements.message.textContent = text
  elements.message.classList.toggle("error", error)
  elements.message.hidden = false
  messageTimer = window.setTimeout(() => {
    elements.message.hidden = true
  }, 5000)
}

function errorText(error) {
  if (typeof error === "string") return error
  if (error instanceof Error) return error.message
  return "操作失败，请稍后重试"
}

function formatNumber(value, maximumFractionDigits = 2) {
  const number = Number(value)
  if (!Number.isFinite(number)) return "-"
  return number.toLocaleString("zh-CN", { maximumFractionDigits })
}

function formatRefreshTime(value) {
  if (!value) return "已连接"
  const raw = String(value)
  const unixSeconds = raw.startsWith("unix:") ? Number(raw.slice(5)) : Number.NaN
  const date = Number.isFinite(unixSeconds) ? new Date(unixSeconds * 1000) : new Date(raw)
  if (Number.isNaN(date.getTime())) return "已刷新"
  return `刷新 ${date.toLocaleString("zh-CN", { hour12: false })}`
}

function statusLabel(value) {
  if (value === "available") return "可用"
  if (value === "degraded") return "降级"
  if (value === "unavailable") return "不可用"
  return "待确认"
}

function badge(text, tone = "") {
  const node = document.createElement("span")
  node.className = `pill ${tone}`.trim()
  node.textContent = text
  return node
}

function renderModels(models, statuses) {
  const statusByModel = new Map((statuses || []).map((item) => [item.model, item]))
  const rows = []
  for (const model of models || []) {
    const row = document.createElement("tr")
    const modelCell = document.createElement("td")
    const code = document.createElement("code")
    code.textContent = model.id
    code.title = model.id
    modelCell.append(code)

    const state = statusByModel.get(model.id)
    const statusCell = document.createElement("td")
    statusCell.append(badge(statusLabel(state?.status), state?.status || ""))

    const testCell = document.createElement("td")
    const testButton = document.createElement("button")
    testButton.type = "button"
    testButton.className = "test-button"
    testButton.textContent = "测试"
    testButton.title = `测试 ${model.id} 的实际请求连通性`
    testButton.addEventListener("click", async () => {
      testButton.disabled = true
      testButton.textContent = "测试中…"
      testButton.classList.remove("success", "failure")
      try {
        const result = await invoke("test_model", { model: model.id })
        if (result.success) {
          testButton.textContent = `成功 ${formatNumber(result.latencyMs, 0)}ms`
          testButton.classList.add("success")
          showMessage(`${model.id} 测试成功，延迟 ${formatNumber(result.latencyMs, 0)}ms`)
        } else {
          testButton.textContent = "失败"
          testButton.classList.add("failure")
          showMessage(`${model.id}：${result.detail}`, true)
        }
      } catch (error) {
        testButton.textContent = "失败"
        testButton.classList.add("failure")
        showMessage(errorText(error), true)
      } finally {
        testButton.disabled = false
      }
    })
    testCell.append(testButton)

    row.append(modelCell, statusCell, testCell)
    rows.push(row)
  }
  if (!rows.length) {
    const row = document.createElement("tr")
    const cell = document.createElement("td")
    cell.colSpan = 3
    cell.textContent = "当前分组没有可用模型"
    cell.className = "muted"
    row.append(cell)
    rows.push(row)
  }
  elements.modelRows.replaceChildren(...rows)
}

function renderDashboard(dashboard) {
  window.__currentDashboard = dashboard
  setView("home")
  elements.homeView.hidden = false
  elements.rechargeView.hidden = true
  elements.setupView.hidden = true
  elements.dashboardView.hidden = false
  elements.refreshButton.hidden = false
  elements.disconnectButton.hidden = false
  elements.connectionLabel.textContent = `${dashboard.keyName || "API Key"} · ${dashboard.keyPrefix || "已连接"}`
  elements.dashboardKey.textContent = `${dashboard.keyName || "API Key"} ${dashboard.keyPrefix || ""}`.trim()
  elements.dashboardUser.textContent = dashboard.userId > 0
    ? `用户 ID ${dashboard.userId} · 系统令牌${dashboard.accessTokenConfigured ? "已保存" : "未配置"}`
    : "用户 ID 未提供"
  elements.dashboardBaseURL.textContent = dashboard.baseUrl
  elements.dashboardBaseURL.title = dashboard.baseUrl
  elements.refreshTime.textContent = formatRefreshTime(dashboard.refreshedAt)
  const balance = dashboard.balance || {}
  elements.remainingPoints.textContent = balance.limited ? formatNumber(balance.remaining_points ?? 0) : "不限额"
  elements.usedPoints.textContent = `${formatNumber(balance.used_points ?? 0)} 积分`
  elements.totalRequests.textContent = formatNumber(dashboard.usage?.total_requests ?? 0, 0)
  elements.last24Requests.textContent = formatNumber(dashboard.usage?.last_24h_requests ?? 0, 0)
  elements.modelCount.textContent = `${dashboard.models?.length ?? 0} 个模型`
  const warnings = [dashboard.statusError].filter(Boolean)
  elements.modelWarning.hidden = warnings.length === 0
  elements.modelWarning.textContent = warnings.join("；")
  elements.workbuddyPath.textContent = dashboard.workbuddyPath
  renderModels(dashboard.models, dashboard.status)
}

function showSetup() {
  setView("home")
  elements.homeView.hidden = false
  elements.rechargeView.hidden = true
  elements.setupView.hidden = false
  elements.dashboardView.hidden = true
  elements.refreshButton.hidden = true
  elements.disconnectButton.hidden = true
  elements.connectionLabel.textContent = "未连接"
  if (!developmentMode) elements.apiKey.value = ""
  elements.workbuddyResult.textContent = ""
}

function setView(view) {
  const recharge = view === "recharge"
  elements.homeView.hidden = recharge
  elements.rechargeView.hidden = !recharge
  elements.navItems.forEach((item) => item.classList.toggle("active", item.dataset.view === view))
}

async function refresh() {
  elements.refreshButton.disabled = true
  elements.refreshButton.textContent = "…"
  try {
    const dashboard = await invoke("refresh_dashboard")
    renderDashboard(dashboard)
  } catch (error) {
    showMessage(errorText(error), true)
  } finally {
    elements.refreshButton.disabled = false
    elements.refreshButton.textContent = "↻"
  }
}

elements.connectForm.addEventListener("submit", async (event) => {
  event.preventDefault()
  const baseUrl = elements.baseURL.value.trim()
  const apiKey = elements.apiKey.value.trim()
  const userId = elements.userId.value.trim()
  const accessToken = elements.accessToken.value.trim()
  if (!baseUrl || !userId || !accessToken) return
  setBusy(elements.connectButton, true, "连接中…", "连接并获取配置")
  try {
    const dashboard = await invoke("connect", {
      baseUrl,
      apiKey,
      userId: userId ? Number(userId) : null,
      accessToken: accessToken || null,
    })
    elements.apiKey.value = ""
    elements.accessToken.value = ""
    renderDashboard(dashboard)
    await refresh()
  } catch (error) {
    showMessage(errorText(error), true)
  } finally {
    setBusy(elements.connectButton, false, "连接中…", "连接并获取配置")
  }
})

elements.refreshButton.addEventListener("click", refresh)

elements.workbuddyButton.addEventListener("click", async () => {
  setBusy(elements.workbuddyButton, true, "写入中…", "导入 WorkBuddy")
  elements.workbuddyResult.textContent = ""
  try {
    const dashboard = await invoke("refresh_dashboard")
    renderDashboard(dashboard)
    const result = await invoke("import_workbuddy")
    elements.workbuddyResult.textContent = `已导入 ${result.modelCount} 个模型，共保留 ${result.totalModelCount} 个${result.backupPath ? "，原配置已备份" : ""}`
    showMessage(`WorkBuddy 配置已更新：${result.path}`)
  } catch (error) {
    showMessage(errorText(error), true)
  } finally {
    setBusy(elements.workbuddyButton, false, "写入中…", "导入 WorkBuddy")
  }
})

elements.disconnectButton.addEventListener("click", async () => {
  try {
    await invoke("clear_profile")
    showSetup()
    showMessage("本地客户端配置已移除")
  } catch (error) {
    showMessage(errorText(error), true)
  }
})

elements.navItems.forEach((item) => {
  item.addEventListener("click", () => setView(item.dataset.view))
})

elements.redeemForm.addEventListener("submit", async (event) => {
  event.preventDefault()
  const code = elements.redeemCode.value.trim()
  if (!code) return
  setBusy(elements.redeemButton, true, "兑换中…", "兑换并刷新额度")
  elements.redeemResult.textContent = ""
  try {
    const result = await invoke("redeem_code", { code })
    const dashboard = await invoke("refresh_dashboard")
    elements.redeemCode.value = ""
    elements.redeemResult.textContent = `兑换成功，已增加 ${formatNumber(result.quota, 0)} 点额度。`
    renderDashboard(dashboard)
    showMessage("兑换成功，账户额度已更新")
  } catch (error) {
    elements.redeemResult.textContent = errorText(error)
    showMessage(errorText(error), true)
  } finally {
    setBusy(elements.redeemButton, false, "兑换中…", "兑换并刷新额度")
  }
})

elements.purchaseButton.addEventListener("click", () => {
  const shopUrl = elements.purchaseButton.dataset.url?.trim()
  if (!shopUrl) {
    elements.purchaseResult.textContent = "云猫寄售正在上架，购买链接开放后会显示在这里。"
    return
  }
  window.open(shopUrl, "_blank", "noopener,noreferrer")
})

async function initialize() {
  elements.developmentBadge.hidden = !developmentMode
  if (developmentMode) {
    elements.baseURL.value = "http://127.0.0.1:3000"
    elements.apiKey.value = "sk-development"
    elements.userId.value = "10001"
    elements.accessToken.value = "dev-system-token"
    elements.setupDescription.textContent = "已启用本地模拟数据，可直接点击连接验证获取配置流程。"
    elements.purchaseResult.textContent = "开发模式：云猫寄售链接尚未配置。"
  }
  if (!tauriInvoke && !developmentMode) {
    showMessage("请通过火灵 API 客户端桌面程序打开此页面", true)
    return
  }
  try {
    const info = await invoke("client_info")
    if (!info.configured) {
      showSetup()
      return
    }
    const dashboard = await invoke("load_dashboard")
    renderDashboard(dashboard)
    await refresh()
  } catch (error) {
    showSetup()
    showMessage(errorText(error), true)
  }
}

void initialize()
