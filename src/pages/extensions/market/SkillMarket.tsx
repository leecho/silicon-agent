import { useCallback, useEffect, useState } from "react";
import { Lightbulb } from "lucide-react";
import { browseSkillMarket, installSkillFromMarket, listSkillCategories } from "../../../api";
import { useNotifications } from "../../../components/ui/NotificationProvider";
import { SkillMarketDrawer } from "./SkillMarketDrawer";
import {
  ListEmpty,
  ListError,
  ListLoading,
  LoadMore,
  MarketCard,
  MarketGrid,
  ResultCount,
  SearchBox,
} from "./ui";
import { useMarketList } from "./useMarketList";
import type { SkillCategory } from "../../../types";

const PAGE_SIZE = 24;

/**
 * **技能市场**（SkillHub）。
 *
 * 它和另外三个市场是各自独立的一套：只有它有**分类**（SkillHub 自己的分类体系）、
 * 有**下载量**、有**正文预览**。专家/团队/插件没有这些东西 ——
 * 硬凑一个通用市场组件，只会让每个货架都填一半字段、留一半空。
 *
 * **7 万+ 技能**：搜索、分页、分类筛选**全部下推到服务端**。
 * 前端过滤那套在这里根本不成立。
 */
export function SkillMarket() {
  const notifications = useNotifications();
  const [query, setQuery] = useState("");
  /** 空串 = 全部。 */
  const [category, setCategory] = useState("");
  const [categories, setCategories] = useState<SkillCategory[]>([]);
  const [installing, setInstalling] = useState<string | null>(null);
  const [openSlug, setOpenSlug] = useState<string | null>(null);

  const fetchPage = useCallback(
    (page: number) => browseSkillMarket(page, PAGE_SIZE, query, category),
    [query, category],
  );
  // 关键词要防抖（每敲一个字打一次服务端太狠）；换分类是点击，无需等待。
  const list = useMarketList(fetchPage, [query, category], query ? 300 : 0);

  // 分类只拉一次（12 个一级分类，不会变）。拉不到不影响浏览——大不了没有分类行。
  useEffect(() => {
    let cancelled = false;
    listSkillCategories()
      .then((cs) => {
        if (!cancelled) setCategories(cs);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  async function install(slug: string, displayName: string) {
    setInstalling(slug);
    try {
      await installSkillFromMarket(slug);
      notifications.notify({
        tone: "success",
        title: "安装成功",
        message: `「${displayName}」已装入「技能」页`,
      });
      setOpenSlug(null);
      list.reload();
    } catch (err) {
      notifications.notify({ tone: "error", title: "安装失败", message: String(err) });
    } finally {
      setInstalling(null);
    }
  }

  return (
    <>

      <SearchBox value={query} onChange={setQuery} placeholder="搜索技能…" />

      {categories.length > 0 && (
        <div className="mb-5 flex flex-wrap items-center gap-1.5">
          <CategoryChip active={category === ""} label="全部" onClick={() => setCategory("")} />
          {categories.map((c) => (
            <CategoryChip
              key={c.key}
              active={category === c.key}
              label={c.name}
              onClick={() => setCategory(c.key)}
            />
          ))}
        </div>
      )}

      {list.loading ? (
        <ListLoading />
      ) : list.error ? (
        <ListError error={list.error} onRetry={list.reload} />
      ) : list.items.length === 0 ? (
        <ListEmpty>{query ? "没有匹配的技能。" : "这个分类下还没有技能。"}</ListEmpty>
      ) : (
        <>
          <ResultCount total={list.total} />
          <MarketGrid>
            {list.items.map((s) => (
              <MarketCard
                key={s.slug}
                icon={Lightbulb}
                title={s.displayName}
                version={s.version}
                description={s.description}
                tags={s.downloads ? [`${s.downloads} 下载`] : []}
                installed={s.installed}
                installing={installing === s.slug}
                onOpen={() => setOpenSlug(s.slug)}
                onInstall={() => void install(s.slug, s.displayName)}
              />
            ))}
          </MarketGrid>
          {list.hasMore && <LoadMore loading={list.loadingMore} onClick={list.loadMore} />}
        </>
      )}

      <SkillMarketDrawer
        slug={openSlug}
        installing={installing === openSlug}
        onClose={() => setOpenSlug(null)}
        onInstall={install}
      />
    </>
  );
}

/** 分类 chip。比货架 Tab 弱一档（无图标、方角），免得两行胶囊分不清层级。 */
function CategoryChip({
  active,
  label,
  onClick,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`rounded-md px-2 py-0.5 text-xs transition ${
        active
          ? "bg-accent font-medium text-foreground"
          : "text-foreground-muted hover:bg-accent hover:text-foreground"
      }`}
    >
      {label}
    </button>
  );
}
