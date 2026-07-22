import { useCallback, useEffect, useRef, useState } from "react";

/** 一页结果。和后端的 `MarketPage<T>` 对齐。 */
export interface Page<T> {
  items: T[];
  total: number;
}

/**
 * 市场列表的**分页 / 搜索行为**。泛型 T = 该市场自己的条目类型，
 * 本 hook **不认识任何货架**——它只知道「给我一页，我拼起来」。
 *
 * 集中在一处而不是抄四份，是因为里面有两件容易写错的事：
 *
 * 1. **竞态**：每次请求带序号，慢响应回来时若已不是最新一次，直接丢弃。
 *    否则在技能货架里快速改关键词，旧结果会盖掉新结果。
 * 2. **防抖**：关键词每敲一个字打一次服务端太狠；但换筛选条件是点击，不该等 300ms。
 */
export function useMarketList<T>(
  /** 取一页。**必须用 useCallback 包**，否则每次渲染都会重新取。 */
  fetchPage: (page: number) => Promise<Page<T>>,
  /** 这些值一变就回到第一页重取（关键词、分类…）。 */
  deps: unknown[],
  /** 关键词类的输入要防抖；点击类的筛选传 0。 */
  debounceMs = 0,
) {
  const [items, setItems] = useState<T[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const seq = useRef(0);

  const load = useCallback(
    async (targetPage: number) => {
      const mine = ++seq.current;
      if (targetPage === 1) {
        setLoading(true);
        setError(null);
      } else {
        setLoadingMore(true);
      }
      try {
        const r = await fetchPage(targetPage);
        // 已经不是最新一次请求了 —— 丢弃，别拿旧结果盖掉新结果。
        if (mine !== seq.current) return;
        setTotal(r.total);
        setItems((prev) => (targetPage === 1 ? r.items : [...prev, ...r.items]));
        setPage(targetPage);
      } catch (err) {
        if (mine !== seq.current) return;
        setError(String(err));
        if (targetPage === 1) setItems([]);
      } finally {
        if (mine === seq.current) {
          setLoading(false);
          setLoadingMore(false);
        }
      }
    },
    [fetchPage],
  );

  useEffect(() => {
    const t = setTimeout(() => void load(1), debounceMs);
    return () => clearTimeout(t);
    // fetchPage 由调用方 useCallback 绑定 deps，故 deps 变化即意味着 fetchPage 变化。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [load, debounceMs, ...deps]);

  return {
    items,
    total,
    loading,
    loadingMore,
    error,
    /** 还有下一页吗。 */
    hasMore: items.length < total,
    loadMore: () => void load(page + 1),
    reload: () => void load(1),
  };
}
