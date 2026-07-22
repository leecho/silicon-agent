import { useEffect, useState } from "react";
import { Crown, GraduationCap, Users } from "lucide-react";
import { teamMarketDetail } from "../../../api";
import { DetailDescription, DetailSection, DetailShell, LandingNote, type DetailRow } from "./ui";
import type { TeamMarketDetail } from "../../../types";

/** **团队**详情：主理人 + 成员。这是团队独有的形状。 */
export function TeamMarketDrawer({
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
  const [detail, setDetail] = useState<TeamMarketDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDetail(null);
    setError(null);
    if (!name) return;

    let cancelled = false;
    teamMarketDetail(name)
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

  // 主理人排在最前，且要标出来——它是团队的入口成员，和普通成员不是一回事。
  const rows: DetailRow[] = detail
    ? detail.lead
      ? [
          { label: detail.lead, hint: "主理人", icon: Crown },
          ...detail.members.map((m) => ({ label: m, icon: GraduationCap })),
        ]
      : detail.members.map((m) => ({ label: m, icon: GraduationCap }))
    : [];

  return (
    <DetailShell
      open={Boolean(name)}
      icon={Users}
      title={detail?.displayName ?? name ?? "团队详情"}
      kindLabel="团队"
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
          <DetailSection icon={Users} title="团队成员" rows={rows} />
        </>
      )}
    </DetailShell>
  );
}
