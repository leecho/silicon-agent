import { useState } from "react";
import { Blocks, GraduationCap, Lightbulb, Users, type LucideIcon } from "lucide-react";
import { ExpertMarket } from "./ExpertMarket";
import { PluginMarket } from "./PluginMarket";
import { SkillMarket } from "./SkillMarket";
import { TeamMarket } from "./TeamMarket";
import type { MarketShelf } from "../../../types";

/**
 * 「扩展 → 市场」Tab。
 *
 * **它只做一件事：切货架。** 它不知道技能有分类、专家有专属技能、团队有主理人 ——
 * 那些是各市场自己的事。四个市场各自持有自己的状态、字段、文案、抽屉与安装器，
 * 彼此之间没有共用的条目类型。
 *
 * 加一个货架 = 加一个组件 + 在下面的表里加一行，不动任何既有市场。
 */

const TABS: { shelf: MarketShelf; icon: LucideIcon; label: string }[] = [
  { shelf: "plugin", icon: Blocks, label: "插件" },
  { shelf: "skill", icon: Lightbulb, label: "技能" },
  { shelf: "expert", icon: GraduationCap, label: "专家" },
  { shelf: "team", icon: Users, label: "团队" },
];

export function MarketTab() {
  const [shelf, setShelf] = useState<MarketShelf>("skill");

  return (
    <div className="h-full overflow-auto p-6 text-sm">
      <div className="mx-auto max-w-[860px]">
        {/* 货架切换在最上层：它决定「你在逛哪个市场」，是搜索与分类的前提。
            不显示各货架计数——技能货架 7 万+，为了一个数字去打一次服务端不值当。 */}
        <div className="mb-3 mt-4 flex flex-wrap items-center justify-center gap-1.5">
          {TABS.map(({ shelf: k, icon: Icon, label }) => (
            <button
              key={k}
              type="button"
              onClick={() => setShelf(k)}
              className={`flex items-center gap-1.5 rounded-md border px-4 py-2 text-sm transition ${
                shelf === k
                  ? "border-primary bg-primary text-primary-foreground"
                  : "border-border-subtle bg-surface text-foreground-secondary hover:border-border hover:text-foreground"
              }`}
            >
              <Icon className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
              {label}
            </button>
          ))}
        </div>

        {/* `key` 让切货架时**整个卸载重建**：搜索词、分类、翻到第几页都属于上一个货架，
            带过去只会是错的（技能的分类 key 在专家市场根本不存在）。 */}
        {shelf === "plugin" && <PluginMarket key="plugin" />}
        {shelf === "skill" && <SkillMarket key="skill" />}
        {shelf === "expert" && <ExpertMarket key="expert" />}
        {shelf === "team" && <TeamMarket key="team" />}
      </div>
    </div>
  );
}
