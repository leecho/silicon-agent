import { useCallback, useState } from "react";
import { Users } from "lucide-react";
import { browseTeamMarket, installTeamFromMarket } from "../../../api";
import { useNotifications } from "../../../components/ui/NotificationProvider";
import { TeamMarketDrawer } from "./TeamMarketDrawer";
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
 * **团队市场**（silicon 官方）。
 *
 * 团队的字段和另外三个都不一样：它有**主理人**与**成员**，
 * 没有分类、没有下载量、没有正文。所以它自己一套，不和别人共用条目类型。
 */
export function TeamMarket() {
  const notifications = useNotifications();
  const [query, setQuery] = useState("");
  const [installing, setInstalling] = useState<string | null>(null);
  const [openName, setOpenName] = useState<string | null>(null);

  const fetchPage = useCallback(
    (page: number) => browseTeamMarket(page, PAGE_SIZE, query),
    [query],
  );
  const list = useMarketList(fetchPage, [query], query ? 300 : 0);

  async function install(name: string, displayName: string) {
    setInstalling(name);
    try {
      await installTeamFromMarket(name);
      notifications.notify({
        tone: "success",
        title: "安装成功",
        message: `「${displayName}」已装入「团队」页`,
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

      <SearchBox value={query} onChange={setQuery} placeholder="搜索团队…" />

      {list.loading ? (
        <ListLoading />
      ) : list.error ? (
        <ListError error={list.error} onRetry={list.reload} />
      ) : list.items.length === 0 ? (
        <ListEmpty>{query ? "没有匹配的团队。" : "这里还没有上架团队。"}</ListEmpty>
      ) : (
        <>
          <ResultCount total={list.total} />
          <MarketGrid>
            {list.items.map((t) => (
              <MarketCard
                key={t.name}
                icon={Users}
                title={t.displayName}
                version={t.version}
                description={t.description}
                tags={t.memberCount > 0 ? [`${t.memberCount} 名成员`] : []}
                installed={t.installed}
                installing={installing === t.name}
                onOpen={() => setOpenName(t.name)}
                onInstall={() => void install(t.name, t.displayName)}
              />
            ))}
          </MarketGrid>
          {list.hasMore && <LoadMore loading={list.loadingMore} onClick={list.loadMore} />}
        </>
      )}

      <TeamMarketDrawer
        name={openName}
        installing={installing === openName}
        onClose={() => setOpenName(null)}
        onInstall={install}
      />
    </>
  );
}
