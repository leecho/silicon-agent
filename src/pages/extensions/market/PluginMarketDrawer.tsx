import { useEffect, useState } from "react";
import { Blocks, Cable, GraduationCap, Terminal, Webhook, Wrench } from "lucide-react";
import { pluginMarketDetail } from "../../../api";
import { DetailDescription, DetailFooter, DetailSection, DetailShell } from "./ui";
import type { PluginMarketDetail } from "../../../types";

/**
 * **插件**详情。
 *
 * 插件是四类里唯一**内容异质**的：技能、专家、MCP、命令、钩子都可能有，
 * 所以它的详情要分五节列。也是唯一**装完就在本页**的（不跳去别的页）。
 */
export function PluginMarketDrawer({
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
  const [detail, setDetail] = useState<PluginMarketDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDetail(null);
    setError(null);
    if (!name) return;

    let cancelled = false;
    pluginMarketDetail(name)
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
      icon={Blocks}
      title={detail?.displayName ?? name ?? "插件详情"}
      kindLabel="插件"
      kindTone="neutral"
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
            title="技能"
            rows={detail.skills.map((s) => ({ label: s, icon: Wrench }))}
            footer="启用后模型可按需加载，不用你手动挑。"
          />
          <DetailSection
            icon={GraduationCap}
            title="专家"
            rows={detail.agents.map((a) => ({ label: a, icon: GraduationCap }))}
          />
          <DetailSection
            icon={Cable}
            title="MCP 连接器"
            rows={detail.mcpServers.map((m) => ({ label: m, icon: Cable }))}
            footer="装完在「MCP」Tab 里连接；需要登录的服务点「授权」即可。"
          />
          <DetailSection
            icon={Terminal}
            title="命令"
            rows={detail.commands.map((c) => ({ label: `/${c}`, icon: Terminal }))}
          />
          <DetailSection
            icon={Webhook}
            title="钩子"
            rows={detail.hooks > 0 ? [{ label: `${detail.hooks} 条自动化规则`, icon: Webhook }] : []}
          />
          <DetailFooter author={detail.author} homepage={detail.homepage} />
        </>
      )}
    </DetailShell>
  );
}
