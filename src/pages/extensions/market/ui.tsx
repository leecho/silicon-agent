import type { ReactNode } from "react";
import { Download, Loader2, Search, type LucideIcon } from "lucide-react";
import { Badge } from "../../../components/ui/Badge";
import { Button } from "../../../components/ui/Button";
import { Drawer, DrawerHeader } from "../../../components/ui/Drawer";

/**
 * 市场页的**纯呈现件**。
 *
 * 这里**不含任何货架概念** —— 没有技能、没有专家、没有团队、没有插件。
 * 进来的是「标题、描述、标签、装没装」，出去的是像素。
 *
 * 四个市场（技能/专家/团队/插件）各自拥有自己的页面与抽屉、各自的字段与文案，
 * 只共用这一层。和后端一样：共用传输，不共用领域。
 */

// ---------------------------------------------------------------- 列表页

export function SearchBox({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
}) {
  return (
    <div className="mb-3 flex items-center gap-2 rounded-lg border border-border-subtle bg-surface px-3 py-2">
      <Search className="h-4 w-4 shrink-0 text-foreground-muted" aria-hidden="true" />
      <input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="w-full bg-transparent text-sm outline-none placeholder:text-foreground-muted"
      />
    </div>
  );
}

/** 市场卡片。`tags` 与描述由各市场自己填 —— 它们的字段本就不一样。 */
export function MarketCard({
  icon: Icon,
  title,
  version,
  description,
  tags,
  installed,
  installing,
  onOpen,
  onInstall,
}: {
  icon: LucideIcon;
  title: string;
  version?: string;
  description?: string;
  tags: string[];
  installed: boolean;
  installing: boolean;
  onOpen: () => void;
  onInstall: () => void;
}) {
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpen();
        }
      }}
      className="group relative flex cursor-pointer flex-col justify-between rounded-lg border border-border-subtle bg-surface p-4 transition hover:border-border hover:bg-accent"
    >
      <div className="flex items-start gap-3">
        <div className="grid h-10 w-10 shrink-0 place-items-center rounded-lg border border-border bg-background text-foreground-muted">
          <Icon className="h-5 w-5" aria-hidden="true" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <p className="truncate font-medium text-foreground">{title}</p>
            {version && <span className="shrink-0 text-xs text-foreground-muted">v{version}</span>}
          </div>
          {description && (
            <p className="mt-1 line-clamp-2 text-xs text-foreground-muted">{description}</p>
          )}
        </div>
      </div>
      <div className="flex items-center justify-between gap-3 pt-3">
        <div className="flex flex-wrap gap-1.5">
          {tags.map((t) => (
            <Badge key={t} tone="neutral">
              {t}
            </Badge>
          ))}
        </div>
        {installed ? (
          <span className="shrink-0 text-xs text-foreground-muted">已安装</span>
        ) : (
          <Button
            tone="primary"
            className="shrink-0 px-2 py-0.5 text-xs opacity-0 transition group-hover:opacity-100 group-focus-within:opacity-100"
            disabled={installing}
            onClick={(e) => {
              e.stopPropagation();
              onInstall();
            }}
          >
            <Download className="h-3.5 w-3.5" aria-hidden="true" />
            {installing ? "安装中…" : "安装"}
          </Button>
        )}
      </div>
    </div>
  );
}

export function MarketGrid({ children }: { children: ReactNode }) {
  return <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">{children}</div>;
}

/** 「共 N 个」。由**各市场自己**显示——总数是各家的事，回传给货架 Tab 只会又耦上。 */
export function ResultCount({ total }: { total: number }) {
  if (total <= 0) return null;
  return <p className="mb-2 text-right text-xs text-foreground-muted">共 {total} 个</p>;
}

export function LoadMore({ loading, onClick }: { loading: boolean; onClick: () => void }) {
  return (
    <div className="mt-4 flex justify-center">
      <Button tone="secondary" disabled={loading} onClick={onClick}>
        {loading ? "加载中…" : "加载更多"}
      </Button>
    </div>
  );
}

export function ListLoading() {
  return <p className="px-4 py-12 text-center text-xs text-foreground-muted">加载中…</p>;
}

export function ListError({ error, onRetry }: { error: string; onRetry: () => void }) {
  return (
    <div className="rounded-lg border border-dashed border-border px-4 py-10 text-center">
      <p className="text-xs text-destructive">加载失败：{error}</p>
      <Button tone="secondary" className="mt-3" onClick={onRetry}>
        重试
      </Button>
    </div>
  );
}

export function ListEmpty({ children }: { children: ReactNode }) {
  return (
    <div className="rounded-lg border border-dashed border-border px-4 py-12 text-center">
      <p className="text-xs text-foreground-muted">{children}</p>
    </div>
  );
}

// ---------------------------------------------------------------- 详情抽屉

/**
 * 详情抽屉的**外壳**：头部、滚动区、底部安装按钮。里面装什么由各市场自己决定。
 *
 * `min-w-0`：flex / grid 子元素默认 `min-width:auto`，一个宽表格（技能正文里很常见）
 * 会把整个抽屉顶宽，连外层 overflow 都跟着失效。
 */
export function DetailShell({
  open,
  icon: Icon,
  title,
  kindLabel,
  kindTone,
  version,
  installed,
  loading,
  error,
  installing,
  onClose,
  onInstall,
  children,
}: {
  open: boolean;
  icon: LucideIcon;
  title: string;
  kindLabel: string;
  kindTone: "neutral" | "info";
  version?: string;
  installed: boolean;
  loading: boolean;
  error: string | null;
  installing: boolean;
  onClose: () => void;
  onInstall: () => void;
  children: ReactNode;
}) {
  return (
    <Drawer
      className="bg-popover text-popover-foreground"
      open={open}
      onClose={onClose}
      title={title}
      width="min(720px, 94vw)"
    >
      <DrawerHeader onClose={onClose}>
        <div className="flex min-w-0 items-center gap-3">
          <div className="grid h-10 w-10 shrink-0 place-items-center rounded-lg bg-card text-foreground-secondary">
            <Icon className="h-5 w-5" aria-hidden="true" />
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <h2 className="truncate text-base font-semibold text-foreground">{title}</h2>
              {!loading && !error && (
                <>
                  {version && <Badge tone="neutral">v{version}</Badge>}
                  <Badge tone={kindTone}>{kindLabel}</Badge>
                  {installed && <Badge tone="success">已安装</Badge>}
                </>
              )}
            </div>
          </div>
        </div>
      </DrawerHeader>

      <div className="flex min-h-0 min-w-0 flex-1 flex-col bg-popover">
        <div className="min-h-0 min-w-0 flex-1 overflow-auto px-5 py-4">
          {error ? (
            <div className="rounded-lg border border-dashed border-border px-4 py-10 text-center">
              <p className="text-sm text-destructive">加载详情失败</p>
              <p className="mt-1 text-xs text-foreground-muted [overflow-wrap:anywhere]">{error}</p>
            </div>
          ) : loading ? (
            <div className="grid h-full min-h-[200px] place-items-center text-sm text-foreground-muted">
              <div className="flex items-center gap-2">
                <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
                加载中...
              </div>
            </div>
          ) : (
            children
          )}
        </div>

        {!loading && !error && (
          <div className="shrink-0 border-t border-border-subtle bg-popover px-5 py-3">
            {installed ? (
              <p className="text-center text-xs text-foreground-muted">已安装</p>
            ) : (
              <Button tone="primary" className="w-full" disabled={installing} onClick={onInstall}>
                {installing ? (
                  <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
                ) : (
                  <Download className="h-4 w-4" aria-hidden="true" />
                )}
                {installing ? "安装中…" : "安装"}
              </Button>
            )}
          </div>
        )}
      </div>
    </Drawer>
  );
}

/** 详情里的一段描述。 */
export function DetailDescription({ text }: { text: string }) {
  if (!text) return null;
  return (
    <p className="mb-5 whitespace-pre-wrap text-sm leading-6 text-foreground-secondary [overflow-wrap:anywhere]">
      {text}
    </p>
  );
}

/**
 * 「装完会出现在哪一页」的提示条。
 *
 * 不说清楚，用户会在市场页找不到刚装的东西，以为没装上。
 */
export function LandingNote({
  icon: Icon,
  lands,
  note,
}: {
  icon: LucideIcon;
  lands: string;
  note?: string;
}) {
  return (
    <div className="mb-5 flex items-start gap-3 rounded-lg border border-border-subtle bg-surface px-4 py-3">
      <div className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-border bg-background text-primary">
        <Icon className="h-4 w-4" aria-hidden="true" />
      </div>
      <p className="text-xs leading-5 text-foreground-secondary">
        装完后它会出现在<span className="font-medium text-foreground">{lands}</span>。{note}
      </p>
    </div>
  );
}

export interface DetailRow {
  hint?: string;
  icon: LucideIcon;
  label: string;
}

/** 一类内容：带边框的行列表。为空则整节不渲染。 */
export function DetailSection({
  footer,
  icon: TitleIcon,
  rows,
  title,
}: {
  footer?: string;
  icon: LucideIcon;
  rows: DetailRow[];
  title: string;
}) {
  if (rows.length === 0) return null;
  return (
    <div className="mb-5">
      <h3 className="mb-3 flex items-center gap-1.5 text-sm font-semibold text-foreground">
        <TitleIcon className="h-4 w-4 text-primary" aria-hidden="true" />
        {title}（{rows.length}）
      </h3>
      <ul className="overflow-hidden rounded-lg border border-border-subtle bg-surface">
        {rows.map((row, i) => {
          const RowIcon = row.icon;
          return (
            <li
              key={row.label}
              className={`flex items-center gap-3 px-4 py-3 ${
                i === rows.length - 1 ? "" : "border-b border-border-subtle"
              }`}
            >
              <div className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-border bg-background text-foreground-secondary">
                <RowIcon className="h-4 w-4" aria-hidden="true" />
              </div>
              <p className="min-w-0 flex-1 truncate text-sm font-medium text-foreground">
                {row.label}
              </p>
              {row.hint && <span className="shrink-0 text-xs text-foreground-muted">{row.hint}</span>}
            </li>
          );
        })}
      </ul>
      {footer && <p className="mt-2.5 text-xs text-foreground-muted">{footer}</p>}
    </div>
  );
}

/** 作者 / 来源。 */
export function DetailFooter({
  author,
  homepage,
}: {
  author?: string | null;
  homepage?: string | null;
}) {
  if (!author && !homepage) return null;
  return (
    <div className="mt-6 space-y-1.5 border-t border-border-subtle pt-4 text-xs text-foreground-muted">
      {author && <p>作者：{author}</p>}
      {homepage && <p className="[overflow-wrap:anywhere]">来源：{homepage}</p>}
    </div>
  );
}
