const tauriInvoke = window.__TAURI__?.core?.invoke

const elements = {
  setupView: document.querySelector("#setup-view"),
  dashboardView: document.querySelector("#dashboard-view"),
  connectForm: document.querySelector("#connect-form"),
  connectButton: document.querySelector("#connect-button"),
  baseURL: document.querySelector("#base-url"),
  apiKey: document.querySelector("#api-key"),
  profileFile: document.querySelector("#profile-file"),
  profilePath: document.querySelector("#profile-path"),
  connectionLabel: document.querySelector("#connection-label"),
  refreshButton: document.querySelector("#refresh-button"),
  disconnectButton: document.querySelector("#disconnect-button"),
  dashboardKey: document.querySelector("#dashboard-key"),
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
}

let messageTimer

function invoke(command, args) {
  if (!tauriInvoke) {
    return Promise.reject(new Error("当前页面不在 UpstreamOps Client 桌面运行时中"))
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

    const imageCell = document.createElement("td")
    imageCell.append(badge(model.supports_images ? "允许" : "关闭", model.supports_images ? "available" : ""))

    const toolsCell = document.createElement("td")
    toolsCell.append(badge(model.supports_tool_call ? "支持" : "关闭", model.supports_tool_call ? "available" : ""))

    const defaultsCell = document.createElement("td")
    const defaults = model.defaults || {}
    const values = [`stream=${defaults.stream === true ? "true" : "false"}`]
    if (defaults.temperature != null) values.push(`temperature=${defaults.temperature}`)
    if (defaults.top_p != null) values.push(`top_p=${defaults.top_p}`)
    if (defaults.max_tokens != null) values.push(`max_tokens=${defaults.max_tokens}`)
    if (defaults.reasoning_effort) values.push(`reasoning=${defaults.reasoning_effort}`)
    const defaultsCode = document.createElement("code")
    defaultsCode.textContent = values.join(" · ")
    defaultsCode.title = defaultsCode.textContent
    defaultsCell.append(defaultsCode)

    row.append(modelCell, statusCell, imageCell, toolsCell, defaultsCell)
    rows.push(row)
  }
  if (!rows.length) {
    const row = document.createElement("tr")
    const cell = document.createElement("td")
    cell.colSpan = 5
    cell.textContent = "当前分组没有可用模型"
    cell.className = "muted"
    row.append(cell)
    rows.push(row)
  }
  elements.modelRows.replaceChildren(...rows)
}

function renderDashboard(dashboard) {
  elements.setupView.hidden = true
  elements.dashboardView.hidden = false
  elements.refreshButton.hidden = false
  elements.disconnectButton.hidden = false
  elements.connectionLabel.textContent = `${dashboard.keyName || "API Key"} · ${dashboard.keyPrefix || "已连接"}`
  elements.dashboardKey.textContent = `${dashboard.keyName || "API Key"} ${dashboard.keyPrefix || ""}`.trim()
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
  elements.setupView.hidden = false
  elements.dashboardView.hidden = true
  elements.refreshButton.hidden = true
  elements.disconnectButton.hidden = true
  elements.connectionLabel.textContent = "未连接"
  elements.apiKey.value = ""
  elements.workbuddyResult.textContent = ""
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
  if (!baseUrl || !apiKey) return
  setBusy(elements.connectButton, true, "连接中…", "连接并获取配置")
  try {
    const dashboard = await invoke("connect", { baseUrl, apiKey })
    elements.apiKey.value = ""
    renderDashboard(dashboard)
    await refresh()
  } catch (error) {
    showMessage(errorText(error), true)
  } finally {
    setBusy(elements.connectButton, false, "连接中…", "连接并获取配置")
  }
})

elements.profileFile.addEventListener("change", async () => {
  const file = elements.profileFile.files?.[0]
  if (!file) return
  try {
    const profile = await file.text()
    const dashboard = await invoke("import_profile", { profile })
    renderDashboard(dashboard)
    showMessage("客户端配置已导入")
    await refresh()
  } catch (error) {
    showMessage(errorText(error), true)
  } finally {
    elements.profileFile.value = ""
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

async function initialize() {
  if (!tauriInvoke) {
    showMessage("请通过 UpstreamOps Client 桌面程序打开此页面", true)
    return
  }
  try {
    const info = await invoke("client_info")
    elements.profilePath.textContent = `本地配置：${info.profilePath}`
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
