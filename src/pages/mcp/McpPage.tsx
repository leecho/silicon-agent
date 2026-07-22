import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ChevronDown,
  ChevronRight,
  ExternalLink,
  KeyRound,
  LogIn,
  Pencil,
  Plug,
  PlugZap,
  Plus,
  RefreshCw,
  Trash2,
  Wifi,
} from "lucide-react";
import {
  type McpServerConfig,
  type McpServerStatus,
  type McpToolDef,
  mcpDeleteServer,
  mcpExportJson,
  mcpImportJson,
  mcpListServers,
  mcpListTools,
  mcpOauthAuthorize,
  mcpReconnect,
  mcpServerStatuses,
  mcpSetAutoApprove,
  mcpSetEnabled,
  mcpSetOauthClientId,
} from "../../api";
import {
  Badge,
  Button,
  Drawer,
  DrawerHeader,
  Modal,
  Switch,
  Tooltip,
  useMessages,
} from "../../components/ui";
import { OwnerGroupTitle } from "../extensions/OwnerGroupTitle";
import {
  STATE_DOT,
  STATE_LABEL,
  STATE_TONE,
  displayName,
  friendlyError,
  humanizeToolName,
  needsClientId,
  originLabel,
} from "./mcpCopy";

const PLACEHOLDER = `{
  "mcpServers": {
    "github": {
      "url": "https://api.githubcopilot.com/mcp/",
      "headers": { "Authorization": "Bearer <你的token>" }
    },
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path"]
    }
  }
}`;

/**
 * MCP 页：面向**普通用户**，不是面向写 server 的开发者。
 *
 * 三条规矩（T106 UX）：
 * 1. **需要用户动手的事要主动找上门**——「需要登录」的服务在行内直接给出「登录」按钮，
 *    并在「扩展 → MCP」Tab 上打待办角标（`useMcpNeedsLogin`）：用户装完带 OAuth 的
 *    插件后，没有任何线索知道自己还差「去点一次登录」这一步。
 *    （页首曾有一条汇总横幅，与行内按钮重复，已去掉。）
 * 2. **技术字段不常驻**——参数 schema 与错误原文收进就近的「详细」折叠；
 *    应用 ID（client_id）只在「服务不支持自动登录」这唯一场景下弹框索取。
 *    它们不删（排查要用），但默认不在视野里。
 * 3. **错误说人话**（`friendlyError`）——每条错误都得回答「那我该怎么办」。
 *
 * 登录这条链路后端本来就是全自动的（授权成功 → 自动重连 → 状态事件回流，
 * `manager.rs:oauth_authorize`），过去只是 UI 没表达出来，用户点完不知道发生了什么。
 */
export function McpPage({ embedded = false }: { embedded?: boolean } = {}) {
  const messages = useMessages();
  const [servers, setServers] = useState<McpServerConfig[]>([]);
  const [statuses, setStatuses] = useState<Record<string, McpServerStatus>>({});
  const [jsonText, setJsonText] = useState("");
  const [editorOpen, setEditorOpen] = useState(false);
  const [editorTitle, setEditorTitle] = useState("添加服务");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [tools, setTools] = useState<Record<string, McpToolDef[]>>({});
  const [authUrl, setAuthUrl] = useState<Record<string, string>>({});
  // 应用 ID（OAuth client_id）不常驻界面：只在「服务不支持自动注册」这唯一场景下
  // 弹框索取。授权失败时自动弹出，否则用户看到「去填应用 ID」却找不到能填的地方——死路。
  const [clientIdFor, setClientIdFor] = useState<McpServerConfig | null>(null);
  const [clientIdDraft, setClientIdDraft] = useState("");
  const [savingClientId, setSavingClientId] = useState(false);
  // 哪些服务已被证实「必须手填应用 ID」。
  // 不能靠 `status.error` 判断：DCR 失败是 mcp_oauth_authorize 命令**同步抛出**的，
  // 不会写进 server 状态。不记这一笔，用户关掉弹框后就再也找不到入口。
  const [clientIdNeeded, setClientIdNeeded] = useState<Record<string, boolean>>({});
  const [rowMsg, setRowMsg] = useState<
    Record<string, { tone: "error" | "success" | "info"; text: string }>
  >({});

  function dismissRowMsg(id: string) {
    setRowMsg((prev) => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
  }

  const reload = useCallback(async () => {
    const [srv, sts] = await Promise.all([mcpListServers(), mcpServerStatuses()]);
    setServers(srv);
    setStatuses(Object.fromEntries(sts.map((s) => [s.serverId, s])));
  }, []);

  useEffect(() => {
    void reload();
    const unlisten = listen<McpServerStatus>("mcp_status_event", (e) => {
      setStatuses((prev) => ({ ...prev, [e.payload.serverId]: e.payload }));
      // 授权成功后后端会自动重连并推 connected —— 这时把「正在等你登录」的残留清掉，
      // 用户不用再点任何东西。
      if (e.payload.state === "connected") {
        dismissRowMsg(e.payload.serverId);
        setAuthUrl((prev) => {
          const next = { ...prev };
          delete next[e.payload.serverId];
          return next;
        });
      }
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, [reload]);

  const manualServers = useMemo(() => servers.filter((s) => !s.pluginId), [servers]);
  const pluginServers = useMemo(() => servers.filter((s) => s.pluginId), [servers]);

  const stateOf = useCallback(
    (s: McpServerConfig): McpServerStatus["state"] => statuses[s.id]?.state ?? "disconnected",
    [statuses],
  );

  function closeEditor() {
    setEditorOpen(false);
    setJsonText("");
    setError(null);
  }

  function openAdd() {
    setJsonText("");
    setError(null);
    setEditorTitle("添加服务");
    setEditorOpen(true);
  }

  async function openEdit(s: McpServerConfig) {
    setError(null);
    try {
      const full = await mcpExportJson();
      const parsed = JSON.parse(full) as { mcpServers?: Record<string, unknown> };
      const entry = parsed.mcpServers?.[s.name];
      if (!entry) {
        setError("无法加载该服务配置");
        return;
      }
      setJsonText(JSON.stringify({ mcpServers: { [s.name]: entry } }, null, 2));
      setEditorTitle(`编辑「${s.name}」`);
      setEditorOpen(true);
    } catch (e) {
      setError(String(e));
    }
  }

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await mcpImportJson(jsonText);
      setEditorOpen(false);
      setJsonText("");
      await reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function doFormat() {
    try {
      setJsonText(JSON.stringify(JSON.parse(jsonText), null, 2));
      setError(null);
    } catch (e) {
      setError(`JSON 格式化失败：${String(e)}`);
    }
  }

  async function toggleEnabled(s: McpServerConfig, enabled: boolean) {
    await mcpSetEnabled(s.id, enabled);
    await reload();
  }

  async function toggleAutoApprove(s: McpServerConfig, v: boolean) {
    await mcpSetAutoApprove(s.id, v);
    await reload();
  }

  async function retry(s: McpServerConfig) {
    dismissRowMsg(s.id);
    setRowMsg((prev) => ({ ...prev, [s.id]: { tone: "info", text: "正在重新连接…" } }));
    try {
      await mcpReconnect(s.id);
      // 结果由 mcp_status_event 回流，不在这里判定成败。
      window.setTimeout(() => dismissRowMsg(s.id), 2000);
    } catch (e) {
      setRowMsg((prev) => ({ ...prev, [s.id]: { tone: "error", text: String(e) } }));
    }
  }

  async function remove(s: McpServerConfig) {
    const ok = await messages.confirm({
      title: "删除服务",
      message: `确定删除「${displayName(s.name)}」吗？删除后需要重新添加才能恢复。`,
      tone: "warning",
      confirmText: "删除",
    });
    if (!ok) return;
    setError(null);
    try {
      await mcpDeleteServer(s.id);
      dismissRowMsg(s.id);
      await reload();
    } catch (e) {
      setError(`删除失败：${String(e)}`);
    }
  }

  async function authorize(s: McpServerConfig) {
    dismissRowMsg(s.id);
    try {
      const url = await mcpOauthAuthorize(s.id);
      setAuthUrl((prev) => ({ ...prev, [s.id]: url }));
      setRowMsg((prev) => ({
        ...prev,
        [s.id]: {
          tone: "info",
          text: "已在浏览器中打开登录页面，完成后将自动连接。",
        },
      }));
    } catch (e) {
      const raw = String(e);
      const { title, hint } = friendlyError(raw);
      setRowMsg((prev) => ({
        ...prev,
        [s.id]: { tone: "error", text: hint ? `${title}——${hint}` : title },
      }));
      // 服务不支持自动注册 → 唯一出路是手填应用 ID，此时（也只有此时）弹框索取。
      if (needsClientId(raw)) {
        setClientIdNeeded((prev) => ({ ...prev, [s.id]: true }));
        // 顺手展开该行：用户关掉弹框后，展开区里的「填写应用 ID」按钮就在眼前，不会失联。
        setExpanded(s.id);
        openClientIdModal(s);
      }
    }
  }

  function openClientIdModal(s: McpServerConfig) {
    setClientIdDraft(s.oauthClientId ?? "");
    setClientIdFor(s);
  }

  async function saveClientId() {
    const s = clientIdFor;
    if (!s) return;
    const value = clientIdDraft.trim();
    setSavingClientId(true);
    try {
      await mcpSetOauthClientId(s.id, value || null);
      await reload();
      setClientIdFor(null);
      setRowMsg((prev) => ({
        ...prev,
        [s.id]: {
          tone: "success",
          text: value ? "应用 ID 已保存，可重新登录。" : "已清除应用 ID。",
        },
      }));
    } catch (e) {
      setRowMsg((prev) => ({ ...prev, [s.id]: { tone: "error", text: `保存失败：${String(e)}` } }));
    } finally {
      setSavingClientId(false);
    }
  }

  async function toggleExpand(s: McpServerConfig) {
    if (expanded === s.id) {
      setExpanded(null);
      return;
    }
    setExpanded(s.id);
    if (tools[s.id]) return;
    try {
      const list = await mcpListTools(s.id);
      setTools((prev) => ({ ...prev, [s.id]: list }));
    } catch {
      setTools((prev) => ({ ...prev, [s.id]: [] }));
    }
  }

  function renderRow(s: McpServerConfig, readOnly: boolean, isLast: boolean) {
    const st = statuses[s.id];
    const state = stateOf(s);
    const isOpen = expanded === s.id;
    const toolList = tools[s.id] ?? [];
    const msg = rowMsg[s.id];
    const friendly = st?.error ? friendlyError(st.error) : null;
    const needsFill =
      s.transport.type !== "stdio" && (clientIdNeeded[s.id] || needsClientId(st?.error));

    return (
      <li key={s.id} className={`group ${isLast ? "" : "border-b border-border-subtle"}`}>
        <div className="flex flex-wrap items-center gap-3.5 px-4 py-4 transition-colors hover:bg-accent">
          <div
            className={`relative grid h-10 w-10 shrink-0 place-items-center rounded-lg border border-border bg-background shadow-sm transition-colors ${
              state === "connected" ? "text-primary" : "text-foreground-muted"
            }`}
          >
            <Plug className="h-5 w-5" aria-hidden="true" />
            <span
              className={`absolute -right-0.5 -top-0.5 h-2.5 w-2.5 rounded-full border border-background ${STATE_DOT[state]}`}
              aria-hidden
            />
          </div>

          <button
            type="button"
            className="min-w-0 flex-1 text-left"
            onClick={() => void toggleExpand(s)}
          >
            <div className="flex items-center gap-2">
              <span className="truncate font-semibold text-foreground">{displayName(s.name)}</span>
              {readOnly && <Badge tone="info">插件</Badge>}
            </div>
            <div className="mt-0.5 flex items-center gap-1.5 truncate text-xs">
              <span className={STATE_TONE[state]}>{STATE_LABEL[state]}</span>
              {state === "connected" && st?.toolCount ? (
                <span className="text-foreground-muted">· {st.toolCount} 项能力</span>
              ) : null}
              <span className="text-foreground-muted">· {originLabel(s)}</span>
            </div>
          </button>

          <div className="flex shrink-0 items-center gap-2.5">
            {/* 登录对插件带来的服务同样开放：它是「给凭证」，不是「管生命周期」。
                只读锁的是启停/编辑/删除，不锁你自己的账号。 */}
            {state === "unauthorized" && (
              <Button tone="primary" className="px-2.5 py-1 text-xs" onClick={() => void authorize(s)}>
                <LogIn className="h-3.5 w-3.5" aria-hidden="true" />
                登录
              </Button>
            )}
            {state === "failed" && (
              <Button
                tone="secondary"
                className="px-2.5 py-1 text-xs"
                onClick={() => void retry(s)}
              >
                <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />
                重试
              </Button>
            )}

            {!readOnly && (
              <>
                {/* 编辑/删除只在 hover 时出现：它们是低频的破坏性操作，常显只是噪音。 */}
                <div className="flex items-center gap-1 opacity-0 transition group-hover:opacity-100 group-focus-within:opacity-100">
                  <Tooltip content="编辑配置">
                    <button
                      type="button"
                      className="grid h-7 w-7 place-items-center rounded-md text-foreground-muted transition hover:bg-surface hover:text-foreground"
                      onClick={() => void openEdit(s)}
                    >
                      <Pencil className="h-4 w-4" />
                    </button>
                  </Tooltip>
                  <Tooltip content="删除">
                    <button
                      type="button"
                      className="grid h-7 w-7 place-items-center rounded-md text-foreground-muted transition hover:bg-surface hover:text-destructive"
                      onClick={() => void remove(s)}
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </Tooltip>
                </div>
                <Tooltip content={s.enabled ? "关闭后 AI 将不再使用该服务" : "开启后 AI 才能使用该服务"}>
                  <span className="flex items-center">
                    <Switch checked={s.enabled} onChange={(v) => void toggleEnabled(s, v)} />
                  </span>
                </Tooltip>
              </>
            )}

            <button
              type="button"
              className="grid h-7 w-7 shrink-0 place-items-center rounded-md text-foreground-muted transition hover:bg-accent hover:text-foreground"
              onClick={() => void toggleExpand(s)}
              title="展开详情"
            >
              {isOpen ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
            </button>
          </div>
        </div>

        {/* 行内提示：授权引导 / 重连进度 / 保存结果 */}
        {msg && (
          <div
            className={`flex items-start justify-between gap-2 border-t border-border-subtle px-4 py-2 text-xs ${
              msg.tone === "error"
                ? "text-destructive"
                : msg.tone === "success"
                  ? "text-success"
                  : "text-foreground-secondary"
            }`}
          >
            <span className="min-w-0 break-words">{msg.text}</span>
            <button
              type="button"
              className="shrink-0 text-foreground-muted transition hover:text-foreground"
              onClick={() => dismissRowMsg(s.id)}
              title="关闭"
            >
              ✕
            </button>
          </div>
        )}

        {isOpen && (
          <div className="space-y-4 border-t border-border-subtle bg-background/40 px-4 py-3.5">
            {/* 出问题时：人话结论 + 下一步 + 直达的动作按钮。
                卡片的出现条件必须把 needsFill 也算上——DCR 失败不写 status.error，
                只判 friendly 会把「填写应用 ID」按钮藏进一张永不渲染的卡片里。 */}
            {(friendly || needsFill) && state !== "connected" && (
              <div className="rounded-md border border-border-subtle bg-surface px-3 py-2.5">
                <p className="text-xs font-medium text-foreground">
                  {friendly?.title ?? "该服务不支持自动登录"}
                </p>
                <p className="mt-0.5 text-xs text-foreground-muted">
                  {friendly?.hint ?? "需在服务方的开发者后台申请应用 ID。"}
                </p>
                {needsFill && (
                  <Button
                    tone="secondary"
                    className="mt-2 px-2 py-0.5 text-xs"
                    onClick={() => openClientIdModal(s)}
                  >
                    <KeyRound className="h-3.5 w-3.5" aria-hidden="true" />
                    填写应用 ID
                  </Button>
                )}
                {st?.error && (
                  <details className="mt-2">
                    <summary className="cursor-pointer text-xs text-foreground-muted transition hover:text-foreground">
                      详细
                    </summary>
                    <p className="mt-1 break-all font-mono text-xs text-destructive">{st.error}</p>
                  </details>
                )}
              </div>
            )}

            {/* 浏览器没弹出来时的唯一退路。 */}
            {state === "unauthorized" && authUrl[s.id] && (
              <div className="flex items-center gap-2">
                <Button
                  tone="ghost"
                  className="px-2 py-1 text-xs"
                  onClick={() => void navigator.clipboard.writeText(authUrl[s.id])}
                >
                  <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
                  复制登录链接
                </Button>
                <span className="text-xs text-foreground-muted">浏览器未自动打开时使用。</span>
              </div>
            )}

            {/* 「自动允许调用」+「能力」合为一个线框容器，内部靠分隔线相连（无缝卡片列表）。
                嵌套 details 必须用**具名** group（group/cap、group/tool），否则外层展开
                会把内层每一个箭头都一起转过来。 */}
            <div className="overflow-hidden rounded-md border border-border-subtle bg-surface">
              {!readOnly && (
                <label className="flex items-center justify-between gap-3 border-b border-border-subtle px-3 py-2.5">
                  <span className="min-w-0">
                    <span className="block text-xs font-medium text-foreground">调用时需确认</span>
                    <span className="mt-0.5 block text-xs text-foreground-muted">
                      开启后，AI 每次使用该服务都会先征得你同意。
                    </span>
                  </span>
                  {/* 开关语义与后端的 autoApprove **相反**：这里勾上=要确认=autoApprove 为假。
                      取反必须同时作用于显示值和写入值，否则标签说的和实际做的正好反着来。 */}
                  <Switch
                    checked={!s.autoApprove}
                    onChange={(needsConfirm) => void toggleAutoApprove(s, !needsConfirm)}
                  />
                </label>
              )}

              <details className="group/cap">
                <summary className="flex cursor-pointer list-none items-center justify-between gap-3 px-3 py-2.5 transition-colors hover:bg-accent [&::-webkit-details-marker]:hidden">
                  <span className="min-w-0">
                    <span className="text-xs font-medium text-foreground">能力</span>
                    <span className="ml-1.5 text-xs text-foreground-muted">{toolList.length}</span>
                  </span>
                  <ChevronRight
                    className="h-4 w-4 shrink-0 text-foreground-muted transition group-open/cap:rotate-90"
                    aria-hidden="true"
                  />
                </summary>

                {toolList.length === 0 ? (
                  <p className="border-t border-border-subtle px-3 py-2.5 text-xs text-foreground-muted">
                    {state === "connected" ? "该服务未提供任何能力。" : "连接后显示。"}
                  </p>
                ) : (
                  <ul className="border-t border-border-subtle bg-background">
                    {toolList.map((t, index, all) => (
                      <li
                        key={t.name}
                        className={index === all.length - 1 ? "" : "border-b border-border-subtle"}
                      >
                        {/* 收起态只留标题；描述、原始工具名、参数 schema 点开才看。 */}
                        <details className="group/tool">
                          <summary className="flex cursor-pointer list-none items-center justify-between gap-2 px-3 py-2 transition-colors hover:bg-accent [&::-webkit-details-marker]:hidden">
                            <span className="min-w-0 truncate text-xs font-medium text-foreground">
                              {humanizeToolName(t.name)}
                            </span>
                            <ChevronRight
                              className="h-3.5 w-3.5 shrink-0 text-foreground-muted transition group-open/tool:rotate-90"
                              aria-hidden="true"
                            />
                          </summary>
                          <div className="space-y-1 border-t border-border-subtle px-3 py-2">
                            {t.description && (
                              <p className="text-xs text-foreground-secondary">{t.description}</p>
                            )}
                            <p className="break-all font-mono text-[11px] text-foreground-muted">
                              {t.name}
                            </p>
                            {t.inputSchema == null ? (
                              <p className="text-[11px] text-foreground-muted">无参数。</p>
                            ) : (
                              <pre className="overflow-auto rounded bg-surface p-2 text-[11px] text-foreground-muted">
                                {JSON.stringify(t.inputSchema, null, 2)}
                              </pre>
                            )}
                          </div>
                        </details>
                      </li>
                    ))}
                  </ul>
                )}
              </details>
            </div>

            {/* 应用 ID 不常驻：只有已设置过的服务才给一个改/清的入口，
                其余情况一律靠授权失败时弹框（openClientIdModal）。 */}
            {s.oauthClientId && (
              <div className="flex items-center justify-between gap-3 text-xs text-foreground-muted">
                <span className="min-w-0 truncate">已设置应用 ID</span>
                <button
                  type="button"
                  className="shrink-0 text-foreground-secondary underline-offset-2 transition hover:text-foreground hover:underline"
                  onClick={() => openClientIdModal(s)}
                >
                  修改
                </button>
              </div>
            )}
          </div>
        )}
      </li>
    );
  }

  return (
    <div className="h-full overflow-auto px-6 py-3 text-sm">
      <section className="mx-auto flex max-w-[860px] flex-col gap-5" aria-label="MCP">
        <div className="mb-1 mt-2 flex items-center justify-between gap-4">
          <div>
            {!embedded && <h1 className="text-xl font-semibold text-foreground">MCP</h1>}
            <p className="mt-1 text-sm text-foreground">
              接入外部服务，供 AI 直接调用。
            </p>
          </div>
          <Button tone="primary" className="shrink-0 px-4" onClick={openAdd}>
            <Plus className="h-4 w-4" />
            添加
          </Button>
        </div>

        {error && !editorOpen && <p className="text-sm text-destructive">{error}</p>}

        <Drawer
          open={editorOpen}
          onClose={closeEditor}
          title={editorTitle}
          widthClassName="w-[min(640px,92vw)]"
        >
          <DrawerHeader onClose={closeEditor}>
            <h2 className="text-sm font-semibold text-foreground">{editorTitle}</h2>
          </DrawerHeader>
          <div className="flex min-h-0 flex-col gap-3 overflow-auto p-5">
            <textarea
              className="h-72 w-full flex-1 resize-none rounded-md border border-border bg-background p-3 font-mono text-xs text-foreground outline-none focus:border-primary"
              spellCheck={false}
              placeholder={PLACEHOLDER}
              value={jsonText}
              onChange={(e) => setJsonText(e.target.value)}
            />
            {error && <p className="text-sm text-destructive">{error}</p>}
            <div className="flex items-center gap-2">
              <Button tone="primary" disabled={busy || !jsonText.trim()} onClick={() => void save()}>
                {busy ? "保存中…" : "保存"}
              </Button>
              <Button tone="ghost" disabled={!jsonText.trim()} onClick={doFormat}>
                格式化
              </Button>
              <Button tone="ghost" onClick={closeEditor}>
                取消
              </Button>
            </div>
          </div>
        </Drawer>

        {/* 应用 ID 弹框：只在「服务不支持自动登录」时出现，不常驻页面。 */}
        <Modal
          open={Boolean(clientIdFor)}
          onClose={() => setClientIdFor(null)}
          title="填写应用 ID"
        >
          <p className="text-sm text-foreground-secondary">
            {clientIdFor ? displayName(clientIdFor.name) : ""} 不支持自动登录，需要你提供一个应用 ID。
          </p>
          <p className="mt-1.5 text-xs text-foreground-muted">
            请到该服务的开发者后台创建应用，把拿到的应用 ID（Client ID）填在下面。
          </p>
          <input
            autoFocus
            value={clientIdDraft}
            onChange={(e) => setClientIdDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void saveClientId();
            }}
            placeholder="例如 abc123..."
            className="mt-3 w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-sm text-foreground outline-none placeholder:font-sans placeholder:text-foreground-muted focus:border-ring"
          />
          <div className="mt-4 flex items-center justify-end gap-2">
            <Button tone="ghost" onClick={() => setClientIdFor(null)}>
              取消
            </Button>
            <Button tone="primary" disabled={savingClientId} onClick={() => void saveClientId()}>
              {savingClientId ? "保存中…" : "保存"}
            </Button>
          </div>
        </Modal>

        {manualServers.length === 0 && pluginServers.length === 0 ? (
          <p className="rounded-lg border border-dashed border-border px-4 py-8 text-center text-xs text-foreground-muted">
            尚未接入任何服务。在「市场」安装插件，其自带的服务将出现在此处。
          </p>
        ) : (
          <div className="flex flex-col gap-6">
            {manualServers.length > 0 && (
              <section>
                <OwnerGroupTitle title="我添加的" count={manualServers.length} />
                <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
                  {manualServers.map((s, index, all) =>
                    renderRow(s, false, index === all.length - 1),
                  )}
                </ul>
              </section>
            )}
            {pluginServers.length > 0 && (
              <section>
                <OwnerGroupTitle
                  title="来自插件"
                  count={pluginServers.length}
                  hint="随插件安装，卸载插件即一并移除"
                />
                <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
                  {pluginServers.map((s, index, all) =>
                    renderRow(s, true, index === all.length - 1),
                  )}
                </ul>
              </section>
            )}
          </div>
        )}
      </section>
    </div>
  );
}
