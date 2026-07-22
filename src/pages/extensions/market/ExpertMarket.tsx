import { useCallback, useState } from "react";
import { GraduationCap } from "lucide-react";
import { browseExpertMarket, installExpertFromMarket } from "../../../api";
import { useNotifications } from "../../../components/ui/NotificationProvider";
import { ExpertMarketDrawer } from "./ExpertMarketDrawer";
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

const PAGE_SIZE = 24;

/**
 * **专家市场**（silicon 官方）。
 *
 * 和技能市场没有任何共用状态：专家没有分类、没有下载量、没有正文预览；
 * 它有的是**专属技能**（只在选中该专家时载入）—— 这是别的货架没有的概念。
 */
export function ExpertMarket() {
  const notifications = useNotifications();
  const [query, setQuery] = useState("");
  const [installing, setInstalling] = useState<string | null>(null);
  const [openName, setOpenName] = useState<string | null>(null);

  const fetchPage = useCallback(
    (page: number) => browseExpertMarket(page, PAGE_SIZE, query),
    [query],
  );
  const list = useMarketList(fetchPage, [query], query ? 300 : 0);

  async function install(name: string, displayName: string) {
    setInstalling(name);
    try {
      await installExpertFromMarket(name);
      notifications.notify({
        tone: "success",
        title: "安装成功",
        message: `「${displayName}」已装入「专家」页`,
      });
      setOpenName(null);
      list.reload();
    } catch (err) {
      notifications.notify({ tone: "error", title: "安装失败", message: String(err) });
    } finally {
      setInstalling(null);
    }
  }

  return (
    <>

      <SearchBox value={query} onChange={setQuery} placeholder="搜索专家…" />

      {list.loading ? (
        <ListLoading />
      ) : list.error ? (
        <ListError error={list.error} onRetry={list.reload} />
      ) : list.items.length === 0 ? (
        <ListEmpty>{query ? "没有匹配的专家。" : "这里还没有上架专家。"}</ListEmpty>
      ) : (
        <>
          <ResultCount total={list.total} />
          <MarketGrid>
            {list.items.map((e) => (
              <MarketCard
                key={e.name}
                icon={GraduationCap}
                title={e.displayName}
                version={e.version}
                description={e.description}
                tags={e.skillCount > 0 ? [`${e.skillCount} 专属技能`] : []}
                installed={e.installed}
                installing={installing === e.name}
                onOpen={() => setOpenName(e.name)}
                onInstall={() => void install(e.name, e.displayName)}
              />
            ))}
          </MarketGrid>
          {list.hasMore && <LoadMore loading={list.loadingMore} onClick={list.loadMore} />}
        </>
      )}

      <ExpertMarketDrawer
        name={openName}
        installing={installing === openName}
        onClose={() => setOpenName(null)}
        onInstall={install}
      />
    </>
  );
}
