import { ArrowRight } from "lucide-react";
import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  clearModelCalls,
  getModelCall,
  getModelCallLogEnabled,
  getModelCallLogStats,
  listModelCalls,
  setModelCallLogEnabled,
} from "../../../api";
import { TextInput } from "../../../components/settings/SettingsControls";
import { useSession } from "../../../components/session/SessionProvider";
import {
  Badge,
  Button,
  Drawer,
  DrawerHeader,
  Select,
  Switch,
  Tabs,
  useMessages,
} from "../../../components/ui";
import type {
  CallLogDetail,
  CallLogFilter,
  CallLogRow,
  CallLogStats,
} from "../../../types";

const USAGE_TYPES = [
  "main_agent",
  "sub_agent",
  "title",
  "suggestion",
  "compaction",
  "curation",
  "other",
];

const USAGE_TYPE_OPTIONS = [
  { label: "全部类型", value: "" },
  ...USAGE_TYPES.map((type) => ({ label: type, value: type })),
];

const STATUS_OPTIONS = [
  { label: "全部状态", value: "" },
  { label: "成功", value: "ok" },
  { label: "失败", value: "error" },
];

type DetailTab = "input" | "output";

/** 模型调用日志：开关 + 筛选 + 列表 + 明细抽屉。默认关闭，开启后记录完整请求/响应。 */
export function CallLogSection() {
  const messages = useMessages();
  const { openSession } = useSession();
  const [enabled, setEnabled] = useState(false);
  const [rows, setRows] = useState<CallLogRow[]>([]);
  const [stats, setStats] = useState<CallLogStats | null>(null);
  const [filter, setFilter] = useState<CallLogFilter>({ limit: 50 });
  const [searchText, setSearchText] = useState("");
  const [detail, setDetail] = useState<CallLogDetail | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setRows(await listModelCalls(filter));
      setStats(await getModelCallLogStats());
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [filter]);

  useEffect(() => {
    getModelCallLogEnabled().then(setEnabled).catch(() => {});
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function toggle(next: boolean) {
    await setModelCallLogEnabled(next);
    setEnabled(next);
  }

  async function clearAll() {
    const confirmed = await messages.confirm({
      title: "清空调用日志",
      message: "将按当前筛选条件删除本机保存的模型调用记录。此操作不可撤销。",
      confirmText: "清空",
      tone: "warning",
    });
    if (!confirmed) return;

    await clearModelCalls(filter);
    await refresh();
  }

  function applySearch() {
    const value = searchText.trim();
    setFilter((current) => ({ ...current, search: value || undefined, offset: undefined }));
  }

  async function openDetail(id: string) {
    try {
      setDetail(await getModelCall(id));
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <section className="flex flex-col gap-5" aria-label="调用日志">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex flex-col cursor-pointer items-start gap-2">
          <div className="flex items-center gap-2">
          <Switch checked={enabled} onChange={(next) => void toggle(next)} />
          
            <span className="text-sm font-medium text-foreground">记录模型调用日志</span>
            
          </div>
          <span className="mt-0.5 block max-w-md text-xs text-foreground-muted">
              开启后在本机完整保存每次调用的请求与响应内容（含敏感 prompt）。默认关闭，
              单条超 256KB 截断、最多保留 5000 条。
            </span>
        </div>
        
      </div>

      <div className="flex flex-wrap gap-2">
        <div className="w-[220px]">
          <TextInput
            placeholder="搜索输入/输出…"
            value={searchText}
            onChange={setSearchText}
            onKeyDown={(event) => {
              if (event.key === "Enter") applySearch();
            }}
          />
        </div>
        <Select
          className="h-12 w-[160px]"
          options={USAGE_TYPE_OPTIONS}
          value={filter.usageType ?? ""}
          onChange={(value) =>
            setFilter((current) => ({ ...current, usageType: value || undefined, offset: undefined }))
          }
        />
        <Select
          className="h-11 w-[140px]"
          options={STATUS_OPTIONS}
          value={filter.status ?? ""}
          onChange={(value) =>
            setFilter((current) => ({ ...current, status: value || undefined, offset: undefined }))
          }
        />
        <Button className="h-9 px-2.5 text-[13px]" tone="outline" onClick={() => void clearAll()}>
            清空
        </Button>
        
      </div>

      {error && <p className="text-sm text-danger">加载失败：{error}</p>}

      {rows.length > 0 && (
        <div className="overflow-x-auto rounded-lg border border-border-subtle">
          <table className="w-full text-left text-sm">
            <thead className="text-xs text-foreground-muted">
              <tr className="border-b border-border-subtle">
                <th className="px-3 py-2">时间</th>
                <th className="px-3 py-2">类型</th>
                <th className="px-3 py-2">模型</th>
                <th className="px-3 py-2 text-right">输入</th>
                <th className="px-3 py-2 text-right">输出</th>
                <th className="px-3 py-2 text-right">缓存</th>
                <th className="px-3 py-2 text-right">耗时</th>
                <th className="px-3 py-2">状态</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr
                  key={row.id}
                  className="cursor-pointer border-b border-border-subtle hover:bg-accent"
                  onClick={() => void openDetail(row.id)}
                >
                  <td className="px-3 py-2 text-foreground-secondary">
                    {formatTime(row.createdAt)}
                  </td>
                  <td className="px-3 py-2">{row.usageType}</td>
                  <td className="px-3 py-2">{row.model}</td>
                  <td className="px-3 py-2 text-right">{row.inputTokens}</td>
                  <td className="px-3 py-2 text-right">{row.outputTokens}</td>
                  <td className="px-3 py-2 text-right">{row.cacheReadTokens}</td>
                  <td className="px-3 py-2 text-right">{row.latencyMs}ms</td>
                  <td className="px-3 py-2">
                    <Badge tone={row.status === "error" ? "danger" : "neutral"}>
                      {row.status}
                    </Badge>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="flex items-center gap-3 px-3 py-1 text-xs text-foreground-muted">
          {stats && (
            <span>
              {stats.count} 条 · {formatBytes(stats.bytes)}
            </span>
          )}
          
        </div>
        </div>
        
      )}

      {loading && rows.length === 0 && (
        <p className="text-sm text-foreground-muted">加载中…</p>
      )}
      {!loading && rows.length === 0 && !error && (
        <div className="rounded-2xl border border-border-subtle bg-surface p-8 text-center">
          <p className="text-sm font-medium text-foreground-secondary">暂无调用记录</p>
          <p className="mt-1 text-xs text-foreground-muted">
            {enabled
              ? "开启后发起几次对话再回来查看。"
              : "调用日志当前关闭；打开上方开关后产生的调用才会记录。"}
          </p>
        </div>
      )}

      <DetailDrawer
        detail={detail}
        onClose={() => setDetail(null)}
        onOpenSession={() => {
          if (!detail?.sessionId) return;
          openSession(detail.sessionId);
          setDetail(null);
        }}
      />
    </section>
  );
}

function DetailDrawer({
  detail,
  onClose,
  onOpenSession,
}: {
  detail: CallLogDetail | null;
  onClose: () => void;
  onOpenSession: () => void;
}) {
  const [tab, setTab] = useState<DetailTab>("input");
  const tabItems = useMemo(
    () => [
      { label: "Input", value: "input" as const },
      { label: "Output", value: "output" as const },
    ],
    [],
  );

  useEffect(() => {
    if (detail) setTab("input");
  }, [detail?.id]);

  if (!detail) return null;

  return (
    <Drawer open={Boolean(detail)} onClose={onClose} title="调用详情" widthClassName="w-[640px] max-w-[90vw]">
      <DrawerHeader onClose={onClose}>
        <div className="flex min-w-0 items-center justify-between gap-3">
          <h3 className="truncate text-sm font-semibold text-foreground">
            {detail.provider || "—"} · {detail.model || "—"}
          </h3>
          <Button
            className="shrink-0 px-2.5 py-1 text-xs"
            disabled={!detail.sessionId}
            tone="outline"
            onClick={onOpenSession}
          >
            打开会话
            <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
          </Button>
        </div>
      </DrawerHeader>
      <div className="flex h-full min-h-0 flex-col overflow-hidden p-5">
        <div className="shrink-0">
          <Field label="状态">
            <span className={detail.status === "error" ? "text-danger" : ""}>
              {detail.status}
              {detail.errorMessage ? ` - ${detail.errorMessage}` : ""}
              {detail.httpStatus ? `（HTTP ${detail.httpStatus}）` : ""}
            </span>
          </Field>
          <Field label="归因">
            {detail.usageType}
            {detail.sessionId ? ` · 会话 ${detail.sessionId}` : ""}
            {detail.expertName ? ` · 专家 ${detail.expertName}` : ""}
          </Field>
          <Field label="Token">
            输入 {detail.inputTokens} · 输出 {detail.outputTokens} · 缓存读{" "}
            {detail.cacheReadTokens} · 缓存写 {detail.cacheCreateTokens}
          </Field>
          <Field label="耗时">
            {detail.latencyMs}ms · finish={detail.finishReason ?? "-"}
          </Field>
        </div>

        <div className="mt-4 flex min-h-0 flex-1 flex-col overflow-hidden">
          <div className="shrink-0 px-2 py-1">
            <Tabs items={tabItems} value={tab} onChange={setTab} />
          </div>
          <div className="flex min-h-0 flex-1 flex-col overflow-hidden py-1">
            {tab === "input" ? (
              <PayloadBlock
                label={`输入 payload${detail.truncated ? "（已截断）" : ""}`}
                text={detail.requestJson}
              />
            ) : (
              <OutputPayload detail={detail} />
            )}
          </div>
        </div>
      </div>
    </Drawer>
  );
}

function OutputPayload({ detail }: { detail: CallLogDetail }) {
  const hasOutput =
    Boolean(detail.reasoningText) ||
    Boolean(detail.responseText) ||
    Boolean(detail.responseToolCallsJson);

  if (!hasOutput) {
    return (
      <p className="p-4 text-center text-sm text-foreground-muted">
        {detail.errorMessage ? "本次调用失败，没有输出内容。" : "没有记录输出内容。"}
      </p>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col gap-3 overflow-auto">
      {detail.reasoningText && <PayloadBlock label="思考" text={detail.reasoningText} />}
      {detail.responseText && <PayloadBlock label="输出文本" text={detail.responseText} />}
      {detail.responseToolCallsJson && (
        <PayloadBlock label="工具调用" text={detail.responseToolCallsJson} />
      )}
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="mb-2 text-sm text-foreground-secondary">
      <span className="text-foreground-muted">{label}：</span>
      {children}
    </div>
  );
}

function PayloadBlock({ label, text }: { label: string; text: string }) {
  const formatted = formatPayload(text);

  return (
    <section className="flex min-h-0 flex-1 border border-border rounded-md px-1 flex-col overflow-hidden">
      <div className="flex shrink-0 items-center justify-between gap-3 px-1 py-2 border-b border-border">
        <h4 className="text-xs font-medium text-foreground-muted">{label}</h4>
        <Badge tone="neutral">{formatBytes(new Blob([formatted]).size)}</Badge>
      </div>
      <pre className="min-h-0 flex-1 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-surface p-3 text-xs leading-5 text-foreground-secondary">
        {formatted}
      </pre>
    </section>
  );
}

function formatPayload(text: string) {
  const trimmed = text.trim();
  if (!trimmed) return "";
  try {
    return JSON.stringify(JSON.parse(trimmed), null, 2);
  } catch {
    return trimmed;
  }
}

function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes)) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function formatTime(epochSeconds: string) {
  const value = Number(epochSeconds);
  if (!Number.isFinite(value)) return "—";
  return new Date(value * 1000).toLocaleString();
}
