import { useCallback, useState } from "react";
import { Blocks } from "lucide-react";
import { browsePluginMarket, installPluginFromMarket } from "../../../api";
import { useNotifications } from "../../../components/ui/NotificationProvider";
import { PluginMarketDrawer } from "./PluginMarketDrawer";
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
 * **插件市场**（标准 plugin 生态）。
 *
 * 插件的内容是**异质**的：技能、专家、MCP、命令、钩子都可能有 ——
 * 所以卡片上用「能力概览」标签，而不是某个单一计数（专家用技能数、团队用成员数）。
 *
 * 官方仓目前没有 plugin 货架，所以这里是空的。插件仍可从本地目录 / zip 安装。
 */
export function PluginMarket() {
  const notifications = useNotifications();
  const [query, setQuery] = useState("");
  const [installing, setInstalling] = useState<string | null>(null);
  const [openName, setOpenName] = useState<string | null>(null);

  const fetchPage = useCallback(
    (page: number) => browsePluginMarket(page, PAGE_SIZE, query),
    [query],
  );
  const list = useMarketList(fetchPage, [query], query ? 300 : 0);

  async function install(name: string, displayName: string) {
    setInstalling(name);
    try {
      await installPluginFromMarket(name);
      notifications.notify({
        tone: "success",
        title: "安装成功",
        message: `「${displayName}」装好了`,
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

      <SearchBox value={query} onChange={setQuery} placeholder="搜索插件…" />

      {list.loading ? (
        <ListLoading />
      ) : list.error ? (
        <ListError error={list.error} onRetry={list.reload} />
      ) : list.items.length === 0 ? (
        <ListEmpty>
          {query
            ? "没有匹配的插件。"
            : "插件市场还没有上架内容。你仍可从本地目录或 zip 安装插件。"}
        </ListEmpty>
      ) : (
        <>
          <ResultCount total={list.total} />
          <MarketGrid>
            {list.items.map((p) => (
              <MarketCard
                key={p.name}
                icon={Blocks}
                title={p.displayName}
                version={p.version}
                description={p.description}
                tags={p.provides}
                installed={p.installed}
                installing={installing === p.name}
                onOpen={() => setOpenName(p.name)}
                onInstall={() => void install(p.name, p.displayName)}
              />
            ))}
          </MarketGrid>
          {list.hasMore && <LoadMore loading={list.loadingMore} onClick={list.loadMore} />}
        </>
      )}

      <PluginMarketDrawer
        name={openName}
        installing={installing === openName}
        onClose={() => setOpenName(null)}
        onInstall={install}
      />
    </>
  );
}
