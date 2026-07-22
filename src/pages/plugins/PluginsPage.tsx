import { useEffect, useState } from "react";
import { Blocks, Plus, Sparkles } from "lucide-react";
import { listPlugins, togglePlugin, uninstallPlugin } from "../../api";
import { Badge } from "../../components/ui/Badge";
import { Button } from "../../components/ui/Button";
import { useSession } from "../../components/session/SessionProvider";
import { useMessages } from "../../components/ui/MessageProvider";
import { useNotifications } from "../../components/ui/NotificationProvider";
import { Switch } from "../../components/ui/Switch";
import { PluginDetailDrawer } from "./PluginDetailDrawer";
import { PluginInstallModal } from "./PluginInstallModal";
import type { Plugin } from "../../types";

// 「使用 AI 创建套件」入口注入 composer 的提示词。套件 = 能力包（技能 和/或 专家，无 type）。
const CREATE_PLUGIN_PROMPT =
  "帮我使用 plugin-creator 创建一个套件（能力包）。套件打包技能（skills/）和/或专家（agents/），无 type 字段——" +
  "启用后其技能进技能列表、其专家进角色选择器。请先问我面向什么角色/场景、需要哪些技能或专家，再据此创作。" +
  "在当前工作目录创作好套件目录（根目录放 plugin.json，技能放 skills/、专家放 agents/）后，" +
  "调用 install_plugin 工具完成登记（会请求我确认）。" +
  "若我想要的是「多专家团队」（lead 派活给成员），告诉我去「团队」页用现有专家组建，或导入团队包。";

/** `embedded`：作为「扩展」页的插件 Tab 内嵌时隐藏自带 h1（与胶囊 Tab 标签重复）。T106 §5.2。 */
export function PluginsPage({ embedded = false }: { embedded?: boolean } = {}) {
  const messages = useMessages();
  const notifications = useNotifications();
  const { enterDraftWithContent } = useSession();
  const [plugins, setPlugins] = useState<Plugin[]>([]);
  const [installOpen, setInstallOpen] = useState(false);
  const [detailId, setDetailId] = useState<string | null>(null);

  async function reload() {
    try {
      setPlugins(await listPlugins());
    } catch (err) {
      notifications.notify({ tone: "error", title: "加载失败", message: String(err) });
    }
  }

  useEffect(() => {
    void reload();
  }, []);

  async function handleToggle(plugin: Plugin) {
    try {
      await togglePlugin(plugin.id, !plugin.enabled);
      await reload();
    } catch (err) {
      notifications.notify({ tone: "error", title: "操作失败", message: String(err) });
    }
  }

  async function handleUninstall(plugin: Plugin) {
    const ok = await messages.confirm({
      title: "卸载套件",
      message: `确定卸载套件「${plugin.displayName}」吗？它带来的全部技能也会一并删除，删了就找不回来了。`,
      tone: "warning",
      confirmText: "卸载",
    });
    if (!ok) return;
    try {
      await uninstallPlugin(plugin.id);
      await reload();
    } catch (err) {
      notifications.notify({ tone: "error", title: "卸载失败", message: String(err) });
    }
  }

  return (
    <div className="h-full overflow-auto px-6 py-3 text-sm">
      <div className="mx-auto max-w-[860px]">
        {/* 头部 */}
        <div className="mb-6 flex items-center justify-between gap-4">
          <div>
            {!embedded && <h1 className="text-xl font-semibold text-foreground">插件</h1>}
            <p className="mt-1 text-sm text-foreground">
              打包好的现成能力，装上就能用。
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Button
              tone="primary"
              onClick={() => enterDraftWithContent(CREATE_PLUGIN_PROMPT)}
            >
              <Sparkles className="h-4 w-4" aria-hidden="true" />
              AI 创建
            </Button>
            <Button tone="secondary" onClick={() => setInstallOpen(true)}>
              <Plus className="h-4 w-4" aria-hidden="true" />
              安装
            </Button>
          </div>
        </div>

        {/* 列表（平铺，套件 = 能力包） */}
        {plugins.length === 0 ? (
          <EmptyState onInstall={() => setInstallOpen(true)} />
        ) : (
          <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
            {plugins.map((plugin, index) => (
              <li
                key={plugin.id}
                className={`group flex items-center gap-3.5 px-4 py-4 transition-colors hover:bg-primary/5 ${
                  index === plugins.length - 1 ? "" : "border-b border-border-subtle"
                }`}
              >
                <div
                  className={`grid h-10 w-10 shrink-0 place-items-center rounded-lg border border-border bg-background shadow-sm transition-colors ${
                    plugin.enabled ? "text-primary" : "text-foreground-muted"
                  }`}
                >
                  <Blocks className="h-5 w-5" aria-hidden="true" />
                </div>

                <button
                  type="button"
                  onClick={() => setDetailId(plugin.id)}
                  className="min-w-0 flex-1 text-left"
                >
                  <div className="flex items-center gap-2">
                    <p className="truncate font-semibold text-foreground">
                      {plugin.displayName}
                    </p>
                    {plugin.skillCount > 0 && (
                      <Badge tone="neutral">{plugin.skillCount} 技能</Badge>
                    )}
                    {plugin.category && <Badge tone="info">{plugin.category}</Badge>}
                  </div>
                  {(plugin.descriptionZh || plugin.description) && (
                    <p className="mt-0.5 line-clamp-1 text-xs text-foreground-secondary">
                      {plugin.descriptionZh || plugin.description}
                    </p>
                  )}
                </button>

                <div className="flex shrink-0 items-center gap-3">
                  {plugin.source === "user" && (
                    <button
                      type="button"
                      onClick={() => void handleUninstall(plugin)}
                      className="rounded-md px-2 py-1 text-xs text-foreground-muted opacity-0 transition hover:bg-accent hover:text-destructive group-hover:opacity-100"
                    >
                      卸载
                    </button>
                  )}
                  <Switch checked={plugin.enabled} onChange={() => void handleToggle(plugin)} />
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>

      <PluginInstallModal
        open={installOpen}
        onClose={() => setInstallOpen(false)}
        onInstalled={() => {
          setInstallOpen(false);
          void reload();
        }}
      />
      <PluginDetailDrawer pluginId={detailId} onClose={() => setDetailId(null)} />
    </div>
  );
}

function EmptyState({ onInstall }: { onInstall: () => void }) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-border py-16 text-foreground-muted">
      <div className="grid h-12 w-12 place-items-center rounded-full bg-muted">
        <Blocks className="h-6 w-6" aria-hidden="true" />
      </div>
      <p className="text-sm">还没有安装套件</p>
      <Button tone="outline" onClick={onInstall}>
        <Plus className="h-4 w-4" aria-hidden="true" />
        安装套件
      </Button>
    </div>
  );
}
