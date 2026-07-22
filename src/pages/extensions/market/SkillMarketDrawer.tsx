import { useEffect, useState } from "react";
import { ChevronRight, Lightbulb, Loader2 } from "lucide-react";
import { skillMarketDetail, skillMarketPreview } from "../../../api";
import { MarkdownText } from "../../../components/ui/MarkdownText";
import { DetailDescription, DetailFooter, DetailShell, LandingNote } from "./ui";
import type { SkillMarketDetail } from "../../../types";

/**
 * **技能**详情（安装前预览）。
 *
 * 只有技能有**正文预览** —— 技能是会**改变模型行为**的东西，而 SkillHub 上的描述
 * 常常只有一句话。装进来之前该看得见它到底写了什么，所以默认展开，不藏进折叠。
 */
export function SkillMarketDrawer({
  slug,
  installing,
  onClose,
  onInstall,
}: {
  slug: string | null;
  installing: boolean;
  onClose: () => void;
  onInstall: (slug: string, displayName: string) => void;
}) {
  const [detail, setDetail] = useState<SkillMarketDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** 正文与详情**分开取**：正文要多拉两跳（列文件 + 取文件），不该让详情陪着一起等。 */
  const [body, setBody] = useState<string | null>(null);
  const [bodyError, setBodyError] = useState<string | null>(null);

  useEffect(() => {
    setDetail(null);
    setError(null);
    setBody(null);
    setBodyError(null);
    if (!slug) return;

    let cancelled = false;
    skillMarketDetail(slug)
      .then((d) => {
        if (!cancelled) setDetail(d);
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      });
    skillMarketPreview(slug)
      .then((text) => {
        if (!cancelled) setBody(text);
      })
      .catch((err) => {
        if (!cancelled) setBodyError(String(err));
      });

    return () => {
      cancelled = true;
    };
  }, [slug]);

  return (
    <DetailShell
      open={Boolean(slug)}
      icon={Lightbulb}
      title={detail?.displayName ?? slug ?? "技能详情"}
      kindLabel="技能"
      kindTone="info"
      version={detail?.version}
      installed={detail?.installed ?? false}
      loading={!detail && !error}
      error={error}
      installing={installing}
      onClose={onClose}
      onInstall={() => detail && slug && onInstall(slug, detail.displayName)}
    >
      {detail && (
        <>
          <DetailDescription text={detail.description} />

          <details open className="group/body mb-5">
            <summary className="mb-3 flex cursor-pointer list-none items-center gap-1.5 text-sm font-semibold text-foreground">
              <ChevronRight
                className="h-4 w-4 shrink-0 text-primary transition-transform group-open/body:rotate-90"
                aria-hidden="true"
              />
              技能内容
            </summary>
            {bodyError ? (
              <p className="rounded-lg border border-dashed border-border px-4 py-6 text-center text-xs text-foreground-muted">
                正文加载失败：{bodyError}
              </p>
            ) : body === null ? (
              <div className="flex items-center gap-2 rounded-lg border border-border-subtle bg-surface px-4 py-6 text-xs text-foreground-muted">
                <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
                加载正文…
              </div>
            ) : (
              // `overflow-hidden` + `min-w-0`：把宽内容（表格、长代码块）关在盒子里。
              // MarkdownText 自己给表格套了 overflow-x-auto，但那只在祖先没被撑破时才有用。
              <div className="min-w-0 overflow-hidden rounded-lg py-3">
                <MarkdownText value={body} className="max-w-full [overflow-wrap:anywhere]" />
              </div>
            )}
          </details>

          <DetailFooter author={detail.author} homepage={detail.homepage} />
        </>
      )}
    </DetailShell>
  );
}
