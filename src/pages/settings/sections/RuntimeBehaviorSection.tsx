import { useCallback, useEffect, useState } from "react";
import { GitBranch, RefreshCcw, Scissors } from "lucide-react";
import {
  getAutoCompactEnabled,
  getAutoCompactThresholdPct,
  getAutoRetryMax,
  getMaxIterations,
  getSubagentExecutionMode,
  getToolParallelism,
  getToolTimeoutSecs,
  setAutoCompactEnabled,
  setAutoCompactThresholdPct,
  setAutoRetryMax,
  setMaxIterations,
  setSubagentExecutionMode,
  setToolParallelism,
  setToolTimeoutSecs,
  type SubagentExecutionMode,
} from "../../../api";
import { Select, Switch, useNotifications } from "../../../components/ui";
import { SettingItem } from "../../../components/settings/SettingsControls";

const RETRY_OPTIONS = [
  { label: "关闭", value: "0" },
  { label: "1 次", value: "1" },
  { label: "2 次", value: "2" },
  { label: "3 次", value: "3" },
  { label: "4 次", value: "4" },
  { label: "5 次", value: "5" },
];

const MAX_ITERATION_OPTIONS = [
  { label: "8 次", value: "8" },
  { label: "12 次", value: "12" },
  { label: "16 次", value: "16" },
  { label: "24 次", value: "24" },
  { label: "32 次", value: "32" },
  { label: "48 次", value: "48" },
  { label: "64 次", value: "64" },
];

const TOOL_TIMEOUT_OPTIONS = [
  { label: "5 秒", value: "5" },
  { label: "10 秒", value: "10" },
  { label: "30 秒", value: "30" },
  { label: "60 秒", value: "60" },
  { label: "120 秒", value: "120" },
  { label: "300 秒", value: "300" },
  { label: "600 秒", value: "600" },
  { label: "1800 秒", value: "1800" },
];

const TOOL_PARALLELISM_OPTIONS = [
  { label: "1（串行）", value: "1" },
  { label: "2", value: "2" },
  { label: "4", value: "4" },
  { label: "8", value: "8" },
  { label: "16", value: "16" },
  { label: "32", value: "32" },
];

const COMPACT_THRESHOLD_OPTIONS = [
  { label: "70%", value: "70" },
  { label: "75%", value: "75" },
  { label: "80%", value: "80" },
  { label: "85%", value: "85" },
  { label: "90%", value: "90" },
  { label: "95%", value: "95" },
];

const SUBAGENT_EXECUTION_MODE_OPTIONS: Array<{
  label: string;
  value: SubagentExecutionMode;
}> = [
  { label: "并行", value: "parallel" },
  { label: "串行", value: "serial" },
];

/** 运行设置：助手如何推进任务（迭代/压缩/子代理/重试）。 */
export function RuntimeBehaviorSection() {
  const notify = useNotifications();
  const [autoCompactOn, setAutoCompactOn] = useState(true);
  const [autoCompactThresholdPct, setAutoCompactThresholdPctState] = useState(90);
  const [autoRetryMax, setAutoRetryMaxState] = useState(3);
  const [maxIterations, setMaxIterationsState] = useState(24);
  const [toolTimeoutSecs, setToolTimeoutSecsState] = useState(30);
  const [toolParallelism, setToolParallelismState] = useState(8);
  const [subagentExecutionMode, setSubagentExecutionModeState] =
    useState<SubagentExecutionMode>("parallel");

  const reload = useCallback(async () => {
    const [
      autoCompactEnabled,
      compactThresholdPct,
      retryMax,
      maxIterationCount,
      toolTimeoutCount,
      toolParallelCount,
      subagentMode,
    ] = await Promise.all([
      getAutoCompactEnabled(),
      getAutoCompactThresholdPct(),
      getAutoRetryMax(),
      getMaxIterations(),
      getToolTimeoutSecs(),
      getToolParallelism(),
      getSubagentExecutionMode(),
    ]);
    setAutoCompactOn(autoCompactEnabled);
    setAutoCompactThresholdPctState(compactThresholdPct);
    setAutoRetryMaxState(retryMax);
    setMaxIterationsState(maxIterationCount);
    setToolTimeoutSecsState(toolTimeoutCount);
    setToolParallelismState(toolParallelCount);
    setSubagentExecutionModeState(subagentMode);
  }, []);

  useEffect(() => {
    reload().catch((err) => notify.error({ title: "加载失败", message: String(err) }));
  }, [reload, notify]);

  async function toggleAutoCompact(value: boolean) {
    setAutoCompactOn(value);
    try {
      await setAutoCompactEnabled(value);
    } catch (err) {
      notify.error({ title: "自动压缩设置失败", message: String(err) });
      setAutoCompactOn(!value);
    }
  }

  async function changeAutoRetry(value: string) {
    const next = Number(value);
    setAutoRetryMaxState(next);
    try {
      await setAutoRetryMax(next);
    } catch (err) {
      notify.error({ title: "自动重试设置失败", message: String(err) });
      await reload();
    }
  }

  async function changeAutoCompactThreshold(value: string) {
    const next = Number(value);
    setAutoCompactThresholdPctState(next);
    try {
      await setAutoCompactThresholdPct(next);
    } catch (err) {
      notify.error({ title: "上下文压缩阈值设置失败", message: String(err) });
      await reload();
    }
  }

  async function changeMaxIterations(value: string) {
    const next = Number(value);
    setMaxIterationsState(next);
    try {
      await setMaxIterations(next);
    } catch (err) {
      notify.error({ title: "最大迭代次数设置失败", message: String(err) });
      await reload();
    }
  }

  async function changeToolTimeoutSecs(value: string) {
    const next = Number(value);
    setToolTimeoutSecsState(next);
    try {
      await setToolTimeoutSecs(next);
    } catch (err) {
      notify.error({ title: "工具超时时长设置失败", message: String(err) });
      await reload();
    }
  }

  async function changeToolParallelism(value: string) {
    const next = Number(value);
    setToolParallelismState(next);
    try {
      await setToolParallelism(next);
    } catch (err) {
      notify.error({ title: "工具并行度设置失败", message: String(err) });
      await reload();
    }
  }

  async function changeSubagentExecutionMode(value: string) {
    const next = value as SubagentExecutionMode;
    setSubagentExecutionModeState(next);
    try {
      await setSubagentExecutionMode(next);
    } catch (err) {
      notify.error({ title: "子代理执行方式设置失败", message: String(err) });
      await reload();
    }
  }

  return (
    <section className="grid gap-8" aria-label="运行设置">
      <div className="settings-section-surface overflow-hidden rounded-lg border border-border bg-surface">
        <SettingItem
          title="自动压缩上下文"
          description="上下文占用接近模型上限时，自动把较早历史压缩成摘要，避免超限。关闭后需手动 /compact。"
          icon={Scissors}
        >
          <Switch checked={autoCompactOn} onChange={(value) => void toggleAutoCompact(value)} />
        </SettingItem>
        {autoCompactOn && (<SettingItem
          title="上下文压缩阈值"
          description="自动压缩开启时，本轮 prompt 用量达到模型上下文上限的该比例后压缩较早历史。"
          icon={Scissors}
        >
          <Select
            className="text-sm h-10 w-28 rounded-lg border border-border bg-background px-3 text-foreground outline-none transition focus:border-ring"
            value={String(autoCompactThresholdPct)}
            tooltip="上下文压缩阈值"
            options={COMPACT_THRESHOLD_OPTIONS}
            onChange={(value) => void changeAutoCompactThreshold(value)}
          />
        </SettingItem>)}

        <SettingItem
          title="最大迭代次数"
          description="单次任务最多允许的模型-工具循环次数，达到上限后会停止继续迭代。"
          icon={RefreshCcw}
        >
          <Select
            className="text-sm h-10 w-28 rounded-lg border border-border bg-background px-3 text-foreground outline-none transition focus:border-ring"
            value={String(maxIterations)}
            tooltip="最大迭代次数"
            options={MAX_ITERATION_OPTIONS}
            onChange={(value) => void changeMaxIterations(value)}
          />
        </SettingItem>
        <SettingItem
          title="工具执行超时（秒）"
          description="全局工具执行的默认超时时长；工具级配置优先于此值。过长可能导致卡顿，过短可能打断正常工作。"
          icon={RefreshCcw}
        >
          <Select
            className="text-sm h-10 w-28 rounded-lg border border-border bg-background px-3 text-foreground outline-none transition focus:border-ring"
            value={String(toolTimeoutSecs)}
            tooltip="工具执行超时（秒）"
            options={TOOL_TIMEOUT_OPTIONS}
            onChange={(value) => void changeToolTimeoutSecs(value)}
          />
        </SettingItem>
        <SettingItem
          title="工具并行度"
          description="一轮内连续只读工具一次最多并发执行的数量。1 = 串行；值越高越快但占用资源更多。"
          icon={RefreshCcw}
        >
          <Select
            className="text-sm h-10 w-28 rounded-lg border border-border bg-background px-3 text-foreground outline-none transition focus:border-ring"
            value={String(toolParallelism)}
            tooltip="工具并行度"
            options={TOOL_PARALLELISM_OPTIONS}
            onChange={(value) => void changeToolParallelism(value)}
          />
        </SettingItem>
        <SettingItem
          title="子代理执行方式"
          description="并行会同时启动同一轮派发的多个子代理；串行会按创建顺序逐个运行，适合资源紧张或任务有隐含先后关系时使用。"
          icon={GitBranch}
        >
          <Select
            className="text-sm h-10 w-28 rounded-lg border border-border bg-background px-3 text-foreground outline-none transition focus:border-ring"
            value={subagentExecutionMode}
            tooltip="子代理执行方式"
            options={SUBAGENT_EXECUTION_MODE_OPTIONS}
            onChange={(value) => void changeSubagentExecutionMode(value)}
          />
        </SettingItem>
        <SettingItem
          title="失败自动重试次数"
          description="模型调用遇到限流、服务端或网络等瞬时错误时自动重试的最大次数。0 表示关闭。"
          icon={RefreshCcw}
        >
          <Select
            className="text-sm h-10 w-28 rounded-lg border border-border bg-background px-3 text-foreground outline-none transition focus:border-ring"
            value={String(autoRetryMax)}
            tooltip="失败自动重试次数"
            options={RETRY_OPTIONS}
            onChange={(value) => void changeAutoRetry(value)}
          />
        </SettingItem>
      </div>
    </section>
  );
}
