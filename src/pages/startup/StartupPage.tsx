import { AlertCircle, Cpu, KeyRound, Loader2, RefreshCw, Settings, Zap } from "lucide-react";
import { Button } from "../../components/ui";

export type StartupStatus = "checking" | "needs-model" | "error";

export function StartupPage({
  errorMessage,
  onConfigure,
  onRetry,
  status,
}: {
  errorMessage?: string | null;
  onConfigure: () => void;
  onRetry: () => void;
  status: StartupStatus;
}) {
  const checking = status === "checking";
  const error = status === "error";

  return (
    <main className="relative grid h-screen place-items-center bg-background px-6 text-foreground">
      <section className="w-full max-w-[620px]" aria-label="启动配置">
        <div className="mb-8 flex items-center gap-3">
          <div className="grid h-11 w-11 shrink-0 place-items-center rounded-lg bg-card text-foreground-secondary">
            {checking ? (
              <Loader2 className="h-5 w-5 animate-spin" aria-hidden="true" />
            ) : error ? (
              <AlertCircle className="h-5 w-5 text-destructive" aria-hidden="true" />
            ) : (
              <Cpu className="h-5 w-5" aria-hidden="true" />
            )}
          </div>
          <div className="min-w-0">
            <h1 className="text-xl font-semibold text-foreground">
              {checking ? "正在检查模型配置" : error ? "模型配置状态读取失败" : "配置模型后开始"}
            </h1>
            <p className="mt-1 text-sm text-foreground-muted">
              {checking
                ? "正在读取本机模型配置。"
                : error
                  ? errorMessage || "无法读取当前模型配置。"
                  : "需要先添加一个可用的 OpenAI-compatible 模型。"}
            </p>
          </div>
        </div>

        {!checking && (
          <>
            <div className="grid gap-3 border-y border-border-subtle py-5">
              <SetupStep
                icon={Cpu}
                title="添加厂商"
                description="填写厂商名称和 Base URL。"
              />
              <SetupStep
                icon={KeyRound}
                title="保存密钥"
                description="API Key 只保存在本机配置中。"
              />
              <SetupStep
                icon={Zap}
                title="启用模型"
                description="添加模型并设为默认模型。"
              />
            </div>

            <div className="mt-7 flex flex-wrap items-center gap-3">
              <Button tone="primary" onClick={onConfigure}>
                <Settings className="h-4 w-4" aria-hidden="true" />
                配置模型
              </Button>
              <Button onClick={onRetry} disabled={checking}>
                <RefreshCw className="h-4 w-4" aria-hidden="true" />
                重新检查
              </Button>
            </div>
          </>
        )}
      </section>
    </main>
  );
}

function SetupStep({
  description,
  icon: Icon,
  title,
}: {
  description: string;
  icon: typeof Cpu;
  title: string;
}) {
  return (
    <div className="grid grid-cols-[32px_minmax(0,1fr)] items-start gap-3">
      <span className="grid h-8 w-8 place-items-center rounded-md bg-card text-foreground-muted">
        <Icon className="h-4 w-4" aria-hidden="true" />
      </span>
      <span className="min-w-0">
        <span className="block text-sm font-semibold text-foreground">{title}</span>
        <span className="mt-0.5 block text-sm text-foreground-muted">{description}</span>
      </span>
    </div>
  );
}
