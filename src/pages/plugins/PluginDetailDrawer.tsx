import { useEffect, useState } from "react";
import { Blocks, Bot, EyeOff, Loader2, Plug, Webhook, Wrench } from "lucide-react";
import { getPluginDetail } from "../../api";
import { avatarEmoji } from "../../lib/avatar";
import { Badge } from "../../components/ui/Badge";
import { Drawer, DrawerHeader } from "../../components/ui/Drawer";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { PluginDetail } from "../../types";
import { SkillDetailDrawer } from "../skills/SkillDetailDrawer";

/** 套件详情抽屉：展示套件元数据 + 其下技能列表（含隐藏的内部知识库技能）。 */
export function PluginDetailDrawer({
  pluginId,
  onClose,
}: {
  pluginId: string | null;
  onClose: () => void;
}) {
  const notifications = useNotifications();
  const [detail, setDetail] = useState<PluginDetail | null>(null);
  const [selectedSkillId, setSelectedSkillId] = useState<string | null>(null);

  useEffect(() => {
    if (!pluginId) {
      setDetail(null);
      setSelectedSkillId(null);
      return;
    }
    setSelectedSkillId(null);
    getPluginDetail(pluginId)
      .then(setDetail)
      .catch((err) =>
        notifications.notify({ tone: "error", title: "加载详情失败", message: String(err) }),
      );
  }, [pluginId, notifications]);

  const plugin = detail?.plugin;
  function closeDrawer() {
    setSelectedSkillId(null);
    onClose();
  }

  return (
    <>
      <Drawer
        className="bg-popover text-popover-foreground"
        open={pluginId !== null}
        onClose={closeDrawer}
        title={plugin?.displayName}
        width="min(720px, 94vw)"
      >
        <DrawerHeader onClose={closeDrawer}>
          <div className="flex min-w-0 items-center gap-3">
            <div className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-card text-foreground-secondary">
              <Blocks className="h-5 w-5" aria-hidden="true" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                <h2 className="truncate text-base font-semibold text-foreground">
                  {plugin?.displayName ?? "套件详情"}
                </h2>
                {plugin && (
                  <>
                    <Badge tone={plugin.source === "builtin" ? "info" : "neutral"}>
                      {plugin.source === "builtin" ? "内置" : "用户安装"}
                    </Badge>
                    <Badge tone={plugin.enabled ? "success" : "neutral"}>
                      {plugin.enabled ? "已启用" : "已禁用"}
                    </Badge>
                    {plugin.version && <Badge tone="neutral">v{plugin.version}</Badge>}
                    {plugin.category && <Badge tone="info">{plugin.category}</Badge>}
                  </>
                )}
              </div>
            </div>
          </div>
        </DrawerHeader>

        <div className="min-h-0 overflow-auto bg-popover px-5 py-4">
        {!detail ? (
          <div className="grid h-full min-h-[200px] place-items-center text-sm text-foreground-muted">
            <div className="flex items-center gap-2">
              <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
              加载中...
            </div>
          </div>
        ) : (
          <>
            {(detail.plugin.descriptionZh || detail.plugin.description) && (
              <div className="mb-5">
                <p className="whitespace-pre-wrap text-sm leading-6 text-foreground-secondary [overflow-wrap:anywhere]">
                  {detail.plugin.descriptionZh || detail.plugin.description}
                </p>
                {detail.plugin.customizedFrom && (
                  <p className="mt-2 text-xs text-foreground-muted">
                    改自：{detail.plugin.customizedFrom}
                  </p>
                )}
              </div>
            )}
            {(detail.author ||
              detail.license ||
              detail.homepage ||
              detail.repository ||
              detail.keywords.length > 0) && (
              <div className="mb-5 space-y-1.5 text-xs text-foreground-muted">
                {detail.author && <p>作者：{detail.author}</p>}
                {detail.license && <p>许可证：{detail.license}</p>}
                {detail.homepage && (
                  <p className="[overflow-wrap:anywhere]">主页：{detail.homepage}</p>
                )}
                {detail.repository && (
                  <p className="[overflow-wrap:anywhere]">仓库：{detail.repository}</p>
                )}
                {detail.keywords.length > 0 && (
                  <div className="flex flex-wrap items-center gap-1.5 pt-0.5">
                    {detail.keywords.map((kw) => (
                      <Badge key={kw} tone="neutral">
                        {kw}
                      </Badge>
                    ))}
                  </div>
                )}
              </div>
            )}
            {detail.agents.length > 0 && (
              <div className="mb-5">
                <h3 className="mb-3 text-sm font-semibold text-foreground">
                  自带的专家（{detail.agents.length}）
                </h3>
                <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
                  {detail.agents.map((a, index) => {
                    const border =
                      index === detail.agents.length - 1
                        ? ""
                        : "border-b border-border-subtle";
                    return (
                      <li key={a.id} className={`flex items-start gap-3 px-4 py-3 ${border}`}>
                        <div className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-border bg-background text-[15px] text-foreground-secondary">
                          {avatarEmoji(a.avatar) ? (
                            <span aria-hidden="true">{avatarEmoji(a.avatar)}</span>
                          ) : (
                            <Bot className="h-4 w-4" aria-hidden="true" />
                          )}
                        </div>
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2">
                            <p className="truncate text-sm font-medium text-foreground">
                              {a.displayName || a.name}
                            </p>
                            {a.profession && (
                              <span className="shrink-0 text-xs text-foreground-muted">
                                {a.profession}
                              </span>
                            )}
                            {!a.enabled && <Badge tone="neutral">已禁用</Badge>}
                          </div>
                          {a.description && (
                            <p className="mt-0.5 line-clamp-2 text-xs text-foreground-secondary">
                              {a.description}
                            </p>
                          )}
                        </div>
                      </li>
                    );
                  })}
                </ul>
                <p className="mt-3 text-xs text-foreground-muted">
                  启用后随时可用：在输入框点角色按钮（👥）就能让它来帮你，也可以把它拉进「团队」当成员。
                </p>
              </div>
            )}
            {detail.mcpServers.length > 0 && (
              <div className="mb-5">
                <h3 className="mb-3 flex items-center gap-1.5 text-sm font-semibold text-foreground">
                  <Plug className="h-4 w-4 text-primary" aria-hidden="true" />
                  MCP 服务（{detail.mcpServers.length}）
                </h3>
                <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
                  {detail.mcpServers.map((m, index) => (
                    <li
                      key={m.name}
                      className={`flex items-start gap-3 px-4 py-3 ${index === detail.mcpServers.length - 1 ? "" : "border-b border-border-subtle"}`}
                    >
                      <div className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-border bg-background text-foreground-secondary">
                        <Plug className="h-4 w-4" aria-hidden="true" />
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <p className="truncate text-sm font-medium text-foreground">{m.name}</p>
                          <Badge tone="neutral">{m.transport}</Badge>
                          <Badge tone={m.state === "connected" ? "success" : m.state === "failed" || m.state === "unauthorized" ? "danger" : "neutral"}>
                            {m.state}
                          </Badge>
                        </div>
                        {m.target && (
                          <p className="mt-0.5 truncate text-xs text-foreground-muted [overflow-wrap:anywhere]">
                            {m.target}
                          </p>
                        )}
                      </div>
                    </li>
                  ))}
                </ul>
                <p className="mt-3 text-xs text-foreground-muted">
                  启用套件后这些 MCP 服务会自动连接，其工具在会话中可直接使用。
                </p>
              </div>
            )}
            {detail.hooks.length > 0 && (
              <div className="mb-5">
                <h3 className="mb-3 flex items-center gap-1.5 text-sm font-semibold text-foreground">
                  <Webhook className="h-4 w-4 text-primary" aria-hidden="true" />
                  Hooks（{detail.hooks.length}）
                </h3>
                <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
                  {detail.hooks.map((h, index) => (
                    <li
                      key={`${h.event}-${h.matcher ?? ""}-${index}`}
                      className={`flex items-start gap-3 px-4 py-3 ${index === detail.hooks.length - 1 ? "" : "border-b border-border-subtle"}`}
                    >
                      <div className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-border bg-background text-foreground-secondary">
                        <Webhook className="h-4 w-4" aria-hidden="true" />
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <Badge tone="neutral">{h.event}</Badge>
                          {h.matcher && <Badge tone="neutral">匹配：{h.matcher}</Badge>}
                        </div>
                        <p className="mt-0.5 truncate font-mono text-xs text-foreground-muted [overflow-wrap:anywhere]">
                          {h.command}
                        </p>
                      </div>
                    </li>
                  ))}
                </ul>
                <p className="mt-3 text-xs text-foreground-muted">
                  启用套件后，这些命令会在对应的工具/会话生命周期点自动执行。
                </p>
              </div>
            )}
            {detail.skills.length > 0 && (
              <>
            <h3 className="mb-3 text-sm font-semibold text-foreground">
              技能（{detail.skills.length}）
            </h3>
            {detail.skills.length === 0 ? (
              <div className="rounded-lg border border-dashed border-border px-4 py-8 text-center text-sm text-foreground-muted">
                这个套件还没有带来任何技能
              </div>
            ) : (
              <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
                {detail.skills.map((skill, index) => {
                  const border =
                    index === detail.skills.length - 1 ? "" : "border-b border-border-subtle";
                  const body = (
                    <>
                      <div className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-border bg-background text-foreground-secondary">
                        <Wrench className="h-4 w-4" aria-hidden="true" />
                      </div>
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <p className="truncate text-sm font-medium text-foreground">
                            {skill.name}
                          </p>
                          {!skill.userInvocable && (
                            <Badge tone="neutral">
                              <EyeOff className="h-3 w-3" aria-hidden="true" />
                              内部
                            </Badge>
                          )}
                        </div>
                        {skill.description && (
                          <p className="mt-0.5 line-clamp-2 text-xs text-foreground-secondary">
                            {skill.description}
                          </p>
                        )}
                      </div>
                    </>
                  );
                  return (
                    <li key={skill.id} className={border}>
                      <button
                        type="button"
                        onClick={() => setSelectedSkillId(skill.id)}
                        title="查看技能详情"
                        className="flex w-full items-start gap-3 px-4 py-3 text-left transition-colors hover:bg-accent"
                      >
                        {body}
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
            <p className="mt-3 text-xs text-foreground-muted">
              点一下技能，可以查看它的详细说明。标着「内部」的技能是套件自己后台用的，你不用单独调用它。
            </p>
              </>
            )}
          </>
        )}
        </div>
      </Drawer>
      <SkillDetailDrawer skillId={selectedSkillId} onClose={() => setSelectedSkillId(null)} />
    </>
  );
}
