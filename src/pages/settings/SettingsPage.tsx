import { ArrowLeft } from "lucide-react";
import { WindowDragRegion } from "../../components/layout/WindowDragRegion";
import { getSettingsTab, settingsTabs, type SettingsTabId } from "./settingsTabs";
import { ProviderSection } from "./sections/ProviderSection";
import { PreferencesSection } from "./sections/PreferencesSection";
import { PersonaSection } from "./sections/PersonaSection";
import { AdvanceConfigSection } from "./sections/AdvanceConfigSection";
import { UsageAnalysisSection } from "./sections/UsageAnalysisSection";
import { CallLogSection } from "./sections/CallLogSection";
import { joinClasses } from "../../components/ui/utils";

/**
 * 设置页是独立全屏页面：专属设置导航 + 返回应用 + 设置内容。
 *
 * App 只负责路由到 SettingsPage；tab 状态、导航布局和内容组合收敛在本页面内。
 */
export function SettingsPage({
  activeTab,
  onBack,
  onBackToProviderCatalog,
  onOpenProvider,
  onSelectTab,
  providerId = null,
  providerPresetKey = null,
}: {
  activeTab: SettingsTabId;
  onBack: () => void;
  onBackToProviderCatalog: () => void;
  onOpenProvider: (target: { providerPresetKey?: string | null; providerId?: string | null }) => void;
  onSelectTab: (tab: SettingsTabId) => void;
  providerId?: string | null;
  providerPresetKey?: string | null;
}) {
  const activeItem = getSettingsTab(activeTab);
  let lastGroup = "";

  return (
    <main className="app-theme grid h-screen grid-cols-[280px_minmax(0,1fr)] bg-background text-foreground">
      <nav
        className="relative flex min-h-0 flex-col gap-0.5 overflow-auto border-r border-border-subtle bg-card px-3 py-5"
        aria-label="设置导航"
      >
        <WindowDragRegion className="h-4 w-full"/>
        <button
          type="button"
          onClick={onBack}
          className="mt-4 flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-xs font-medium text-foreground-muted transition hover:bg-accent hover:text-foreground"
        >
          <ArrowLeft className="h-4 w-4" aria-hidden="true" />
          返回应用
        </button>
        {settingsTabs.map((item) => {
          const showGroup = item.group !== lastGroup;
          lastGroup = item.group;
          const isActive = item.id === activeTab;
          return (
            <div key={item.id}>
              {showGroup && (
                <div className="px-3 pb-1.5 pt-3.5 text-[11px] font-bold tracking-wide text-foreground-muted">
                  {item.group}
                </div>
              )}
              <button
                type="button"
                onClick={() => onSelectTab(item.id)}
                className={joinClasses(
                  "flex w-full min-w-0 items-center rounded-lg px-3 py-2 text-left text-sm transition",
                  isActive
                    ? "bg-accent font-semibold text-foreground"
                    : "text-foreground-secondary hover:bg-accent hover:text-foreground"
                )}
              >
                <span className="min-w-0 truncate">{item.label}</span>
              </button>
            </div>
          );
        })}
      </nav>

      <div className="relative flex min-h-0 flex-col overflow-auto bg-background px-10 py-8">
        <WindowDragRegion className="h-4" />
        <div className="mx-auto w-full max-w-3xl">
          <div className="mb-8">
            <h1 className="text-lg font-semibold text-foreground">{activeItem.label}</h1>
            <p className="mt-1 text-sm text-foreground-muted">{activeItem.description}</p>
          </div>
          {activeTab === "model-advance" && <AdvanceConfigSection />}
          {activeTab === "model-provider" && (
            <ProviderSection
              providerId={providerId}
              providerPresetKey={providerPresetKey}
              onBackToCatalog={onBackToProviderCatalog}
              onOpenProvider={onOpenProvider}
            />
          )}
          {activeTab === "usage-analysis" && <UsageAnalysisSection />}
          {activeTab === "call-log" && <CallLogSection />}
          {activeTab === "preferences" && <PreferencesSection />}
          {activeTab === "agent-persona" && <PersonaSection />}
        </div>
      </div>
    </main>
  );
}
