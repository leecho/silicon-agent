import { useEffect, useRef, useState, type RefObject } from "react";
import type { Artifact } from "../../types";
import { artifactFileName, artifactIcon } from "./artifactFilePresentation";

export type MessageFeedNavItem = {
  /** 轮根（user 消息 id）：跳转与 active 判定。 */
  id: string;
  /** 用户消息首行（截断）——卡片标题。 */
  title: string;
  /** 该轮 AI 最终回复预览（截断）——卡片正文；运行中/无回复为空。 */
  reply: string;
  /** 该轮产物（已滤掉 working 中间文件）。 */
  artifacts: Artifact[];
};

const CARD_ARTIFACT_LIMIT = 4;

/**
 * 消息流右缘侧边导航（Codex 风格）：
 * - 默认：一列指示条（每轮一条），按与当前阅读轮的距离渐变（近长亮、远短淡）。
 * - 悬浮某条：弹出该轮卡片——用户消息(标题) + AI 回复(正文) + 产物 chips(点击打开)。
 * - 点击条/卡片标题区：平滑滚动跳转到该轮。
 * 仅当存在用户轮且视口可滚动时渲染。
 */
export function MessageFeedNav({
  scrollRef,
  items,
  onOpenArtifact,
}: {
  /** 滚动视口（用于跳转与当前位置计算）。 */
  scrollRef: RefObject<HTMLDivElement | null>;
  /** 每轮一项，顺序与 feed 一致。 */
  items: MessageFeedNavItem[];
  /** 点击产物 chip：打开该产物（复用 feed 的产物预览）。 */
  onOpenArtifact?: (a: Artifact) => void;
}) {
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [scrollable, setScrollable] = useState(false);

  // items 每次流式 delta 都换新数组（reply 增长），但轮 id 集合只在新增/删除轮时变。
  // 用 ref 取最新、用 id 集合作 effect 依赖：避免每 delta 重装监听/全量 DOM 查询。
  const itemsRef = useRef(items);
  itemsRef.current = items;
  const itemsKey = items.map((it) => it.id).join("|");

  // 监听滚动 / 尺寸变化：重算可滚动性与当前 active 轮。rAF 合并突发事件，避免流式期
  // ResizeObserver 高频触发 + 逐条 querySelector 阻塞主线程。
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;

    const compute = () => {
      const list = itemsRef.current;
      setScrollable(el.scrollHeight > el.clientHeight + 4);
      const threshold = el.getBoundingClientRect().top + 80;
      let current: string | null = list.length > 0 ? list[0].id : null;
      for (const item of list) {
        const node = el.querySelector<HTMLElement>(
          `[data-feed-msg-id="${CSS.escape(item.id)}"]`,
        );
        if (!node) continue;
        if (node.getBoundingClientRect().top <= threshold) {
          current = item.id;
        } else {
          break;
        }
      }
      setActiveId(current);
    };

    let raf = 0;
    const schedule = () => {
      if (raf) return;
      raf = requestAnimationFrame(() => {
        raf = 0;
        compute();
      });
    };

    compute();
    el.addEventListener("scroll", schedule, { passive: true });
    const ro = new ResizeObserver(schedule);
    ro.observe(el);
    return () => {
      if (raf) cancelAnimationFrame(raf);
      el.removeEventListener("scroll", schedule);
      ro.disconnect();
    };
  }, [scrollRef, itemsKey]);

  if (items.length === 0 || !scrollable) return null;

  const jumpTo = (id: string) => {
    const node = scrollRef.current?.querySelector<HTMLElement>(
      `[data-feed-msg-id="${CSS.escape(id)}"]`,
    );
    node?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  const hoveredIndex = hoveredId
    ? items.findIndex((it) => it.id === hoveredId)
    : -1;

  return (
    <div
      role="navigation"
      className="pointer-events-none absolute inset-y-0 right-0 z-10 flex items-center pr-1"
      aria-label="消息导航"
    >
      <div
        className="pointer-events-auto flex flex-col items-end"
        onMouseLeave={() => setHoveredId(null)}
      >
        {items.map((item, index) => {
          let width: number;
          let opacity: number;
          if (hoveredIndex >= 0) {
            // 悬浮：以悬浮项为中心的渐变——中间长且亮，向两端渐短渐暗。
            const distance = Math.abs(index - hoveredIndex);
            width = Math.max(4, 26 - distance * 6);
            opacity = Math.max(0.2, 1 - distance * 0.24);
          } else {
            // 默认：全部短；当前阅读轮略亮作位置提示。
            width = 8;
            opacity = item.id === activeId ? 0.6 : 0.32;
          }
          const hovered = item.id === hoveredId;
          return (
            <div
              key={item.id}
              className="relative flex h-2.5 items-center"
              onMouseEnter={() => setHoveredId(item.id)}
            >
              {hovered && (
                <NavCard
                  item={item}
                  onJump={() => jumpTo(item.id)}
                  onOpenArtifact={onOpenArtifact}
                />
              )}
              <button
                type="button"
                aria-label={item.title}
                title={item.title}
                onClick={() => jumpTo(item.id)}
                className="h-0.5 rounded-full bg-foreground transition-all"
                style={{ width, opacity }}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}

function NavCard({
  item,
  onJump,
  onOpenArtifact,
}: {
  item: MessageFeedNavItem;
  onJump: () => void;
  onOpenArtifact?: (a: Artifact) => void;
}) {
  const shown = item.artifacts.slice(0, CARD_ARTIFACT_LIMIT);
  const overflow = item.artifacts.length - shown.length;
  return (
    <div className="absolute right-full top-1/2 mr-2 w-64 max-w-[70vw] -translate-y-1/2 rounded-lg border border-border bg-popover p-3 text-popover-foreground shadow-lg">
      <button
        type="button"
        onClick={onJump}
        className="block w-full text-left"
      >
        <p className="line-clamp-2 text-sm font-semibold text-foreground">
          {item.title}
        </p>
        {item.reply && (
          <p className="mt-1 line-clamp-3 text-xs leading-5 text-foreground-muted">
            {item.reply}
          </p>
        )}
      </button>
      {shown.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {shown.map((a) => {
            const Icon = artifactIcon(a.path);
            const name = artifactFileName(a.path);
            return (
              <button
                key={a.path}
                type="button"
                title={a.path}
                onClick={() => onOpenArtifact?.(a)}
                className="inline-flex max-w-[130px] items-center gap-1 rounded-md border border-border-subtle bg-surface px-2 py-1 text-xs text-foreground-secondary transition hover:bg-accent hover:text-foreground"
              >
                <Icon className="h-3 w-3 shrink-0" aria-hidden="true" />
                <span className="truncate">{name}</span>
              </button>
            );
          })}
          {overflow > 0 && (
            <span className="inline-flex items-center rounded-md border border-border-subtle bg-surface px-2 py-1 text-xs text-foreground-muted">
              +{overflow}
            </span>
          )}
        </div>
      )}
    </div>
  );
}
