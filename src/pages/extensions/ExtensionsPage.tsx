import { Tabs } from "../../components/ui";
import { ExpertsPage } from "../experts/ExpertsPage";
import { McpPage } from "../mcp/McpPage";
import { useMcpNeedsLogin } from "../mcp/useMcpNeedsLogin";
import { PluginsPage } from "../plugins/PluginsPage";
import { SkillsPage } from "../skills/SkillsPage";
import { TeamsPage } from "../teams/TeamsPage";
import { MarketTab } from "./market/MarketTab";
import { DEFAULT_EXTENSION_TAB, EXTENSION_TABS, type ExtensionTabId } from "./extensionTabs";

/**
 * 「扩展」页：一切扩展的单一入口（原侧栏「技能 / 套件 / 连接器 / 专家」四项收敛于此）。
 *
 * 顶部胶囊 Tab：市场（落地）/ 插件 / 技能 / 专家 / 团队 / MCP。
 *
 * **三体系并行**（T108，取代 T106 的「统一容器」）：
 * - **插件** = 标准生态接入口（Claude / Codex 规范）：技能与专家**全局公开**；
 * - **专家** = silicon 特色：自带技能**私有**，选中该专家时才载入；
 * - **团队** = silicon 特色：自带成员与技能**私有**，激活该团队时才载入。
 *
 * 三者各有各的清单（`plugin.json` / `expert.json` / `team.json`）、各有各的市场货架、
 * 各有各的装载器 —— 装完各归其位。
 *
 * 各子页以 `embedded` 模式渲染：隐去自带 h1（与胶囊 Tab 标签重复）与内部「广场」子 Tab
 * （广场已统一到「市场」Tab）。
 */
export function ExtensionsPage({
  tab,
  onSelectTab,
}: {
  tab?: ExtensionTabId;
  onSelectTab: (tab: ExtensionTabId) => void;
}) {
  const active = tab ?? DEFAULT_EXTENSION_TAB;
  // 有服务在等登录时，给 MCP Tab 打个待办角标——否则用户装完带 OAuth 的插件后
  // 没有任何线索知道自己还差「去点一次登录」这一步，只会觉得「装了但不好使」。
  const needsLogin = useMcpNeedsLogin();
  // Tabs 基元的 label 是 ReactNode，直接把图标和文字一起塞进去。
  const items = EXTENSION_TABS.map(({ icon: Icon, id, label }) => ({
    value: id,
    label: (
      <span className="flex items-center gap-1.5">
        <Icon className="h-4 w-4 shrink-0" aria-hidden="true" />
        {label}
        {id === "mcp" && needsLogin > 0 && (
          <span
            className="grid h-4 min-w-4 shrink-0 place-items-center rounded-full bg-warning px-1 text-[10px] font-semibold leading-none text-background"
            title={`${needsLogin} 个服务需要登录`}
          >
            {needsLogin}
          </span>
        )}
      </span>
    ),
  }));

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden ">
      <div className="session-header shrink-0 border-b border-border-subtle py-2 px-2">
          <Tabs items={items} onChange={onSelectTab} value={active} />
      </div>
      <div className="min-h-0 flex-1">
        {active === "market" && <MarketTab />}
        {active === "plugins" && <PluginsPage embedded />}
        {active === "skills" && (
          <SkillsPage embedded />
        )}
        {active === "experts" && (
          <ExpertsPage embedded />
        )}
        {active === "teams" && <TeamsPage embedded />}
        {active === "mcp" && <McpPage embedded />}
      </div>
    </div>
  );
}
