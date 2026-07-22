import { useEffect, useState } from "react";
import { GraduationCap, Wrench } from "lucide-react";
import { expertMarketDetail } from "../../../api";
import { DetailDescription, DetailSection, DetailShell, LandingNote } from "./ui";
import type { ExpertMarketDetail } from "../../../types";

/** **专家**详情。它有的是专属技能——不是技能那种正文，也不是团队那种成员名单。 */
export function ExpertMarketDrawer({
  name,
  installing,
  onClose,
  onInstall,
}: {
  name: string | null;
  installing: boolean;
  onClose: () => void;
  onInstall: (name: string, displayName: string) => void;
}) {
  const [detail, setDetail] = useState<ExpertMarketDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDetail(null);
    setError(null);
    if (!name) return;

    let cancelled = false;
    expertMarketDetail(name)
      .then((d) => {
        if (!cancelled) setDetail(d);
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [name]);

  return (
    <DetailShell
      open={Boolean(name)}
      icon={GraduationCap}
      title={detail?.displayName ?? name ?? "专家详情"}
      kindLabel="专家"
      kindTone="info"
      version={detail?.version}
      installed={detail?.installed ?? false}
      loading={!detail && !error}
      error={error}
      installing={installing}
      onClose={onClose}
      onInstall={() => detail && name && onInstall(name, detail.displayName)}
    >
      {detail && (
        <>
          <DetailDescription text={detail.description} />
          <DetailSection
            icon={Wrench}
            title="专属技能"
            rows={detail.skills.map((s) => ({ label: s, icon: Wrench }))}
            footer="只在选中该专家时载入，不会占用其他会话的上下文。"
          />
        </>
      )}
    </DetailShell>
  );
}
