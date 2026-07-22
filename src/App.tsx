import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type JSX,
} from "react";
import {
  getAppPlatform,
  getToolLabels,
  listEnabledModels,
  type AppPlatform,
} from "./api";
import { Sidebar } from "./components/layout/Sidebar";
import { WindowDragRegion } from "./components/layout/WindowDragRegion";
import {
  COLLAPSED_SIDEBAR_TITLEBAR_ACTIONS_CLASS_NAME,
  getSidebarLayoutState,
  getTitlebarLayoutState,
  SIDEBAR_WIDTH_PX,
  type SidebarMode,
} from "./components/layout/sidebarLayout";
import { SidebarTitlebarActions } from "./components/layout/SidebarTitlebarActions";
import { SessionSearchModal } from "./components/layout/SessionSearchModal";
import { hasEnabledModels } from "./lib/modelAvailability";
import { applyTheme, type ThemePreference } from "./lib/theme";
import type { AppSection } from "./appNavigation";
import { HomePage } from "./pages/home/HomePage";
import { SessionPage } from "./pages/session/SessionPage";
import { SessionDraftPage } from "./pages/session/SessionDraftPage";
import { setToolLabels } from "./components/session/toolNarrative";
import { useSession } from "./components/session/SessionProvider";
import { SessionAttentionNotificationBridge } from "./components/session/SessionAttentionNotificationBridge";
import { ProjectsPage } from "./pages/projects/ProjectsPage";
import { AgentsPage } from "./pages/agents/AgentsPage";
import { RemotePage } from "./pages/remote/RemotePage";
import { SettingsPage } from "./pages/settings/SettingsPage";
import { ScheduledTasksPage } from "./pages/scheduling/ScheduledTasksPage";
import { ExtensionsPage } from "./pages/extensions/ExtensionsPage";
import { KnowledgeBasesPage } from "./pages/knowledge-bases/KnowledgeBasesPage";
import { StartupPage, type StartupStatus } from "./pages/startup/StartupPage";
import { NotificationProvider } from "./components/ui/NotificationProvider";
import { MessageProvider } from "./components/ui/MessageProvider";
import { SessionProvider } from "./components/session/SessionProvider";
import { TrayEventBridge } from "./components/tray/TrayEventBridge";
import { useAppNavigation, type AppLocation } from "./hooks/useAppNavigation";

type ModelStartupStatus = StartupStatus | "ready";
type SessionChromeStyle = CSSProperties & {
  "--session-body-padding-inline": string;
  "--session-header-padding-left": string;
  "--titlebar-collapsed-actions-left": string;
};

const COLLAPSED_CONTENT_INSET_GAP_PX = 12;

const DEV_APP_PLATFORM_OVERRIDE_KEY = "silicon-worker.dev.appPlatform";

function isDevBuild(): boolean {
  return ((import.meta as ImportMeta & { env?: { DEV?: boolean } }).env?.DEV ?? false) === true;
}

function normalizeAppPlatformCandidate(platform: string | null | undefined): AppPlatform | null {
  return platform === "macos" || platform === "windows" || platform === "linux" || platform === "unknown"
    ? platform
    : null;
}

function getDevAppPlatformOverride(): AppPlatform | null {
  if (!isDevBuild() || typeof window === "undefined") {
    return null;
  }
  const queryPlatform = normalizeAppPlatformCandidate(
    new URLSearchParams(window.location.search).get("sw-platform"),
  );
  if (queryPlatform) {
    return queryPlatform;
  }
  try {
    return normalizeAppPlatformCandidate(localStorage.getItem(DEV_APP_PLATFORM_OVERRIDE_KEY));
  } catch {
    return null;
  }
}

function detectInitialAppPlatform(): AppPlatform {
  const devOverride = getDevAppPlatformOverride();
  if (devOverride) {
    return devOverride;
  }
  if (typeof navigator === "undefined") {
    return "unknown";
  }
  const platform =
    (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform ??
    navigator.platform ??
    navigator.userAgent;
  if (/mac/i.test(platform)) return "macos";
  if (/win/i.test(platform)) return "windows";
  if (/linux/i.test(platform)) return "linux";
  return "unknown";
}

function AppShell() {
  const navigation = useAppNavigation();
  const section = navigation.current.section;
  const [startupStatus, setStartupStatus] = useState<ModelStartupStatus>("checking");
  const [startupError, setStartupError] = useState<string | null>(null);
  useEffect(() => applyTheme((localStorage.getItem("theme") as ThemePreference | null) ?? "system"), []);
  // 工具叙事标签：启动时拉取后端 Tool::label()（单一真相源）注入叙事模块，先于 feed 渲染。
  useEffect(() => {
    void getToolLabels().then(setToolLabels).catch(() => {});
  }, []);

  const refreshStartupStatus = useCallback(async () => {
    setStartupStatus("checking");
    setStartupError(null);
    try {
      const groups = await listEnabledModels();
      setStartupStatus(hasEnabledModels(groups) ? "ready" : "needs-model");
    } catch (err) {
      setStartupError(err instanceof Error ? err.message : String(err));
      setStartupStatus("error");
    }
  }, []);

  useEffect(() => {
    void refreshStartupStatus();
  }, [refreshStartupStatus]);

  function handleSettingsBack() {
    navigation.replace({ section: "session" });
    void refreshStartupStatus();
  }

  const handleSessionOpen = useCallback(
    (target?: { sessionId?: string | null; draftId?: string | null }) => {
      if (target?.draftId) {
        navigation.navigate({ section: "session", draftId: target.draftId });
        return;
      }
      if (target && "sessionId" in target) {
        navigation.navigate({ section: "session", sessionId: target.sessionId ?? null });
        return;
      }
      navigation.navigate({ section: "session" });
    },
    [navigation],
  );

  const handleTrayOpenProject = useCallback(
    (projectId: string) => {
      navigation.navigate({ section: "projects", projectId });
    },
    [navigation],
  );

  const handleTrayOpenAgent = useCallback(
    (agentId: string) => {
      navigation.navigate({ section: "agents", agentId });
    },
    [navigation],
  );

  const settingsTab =
    navigation.current.section === "settings"
      ? navigation.current.tab ?? "model-provider"
      : "model-provider";
  const settingsLocation = navigation.current.section === "settings" ? navigation.current : null;

  return (
    <SessionProvider onOpenSession={handleSessionOpen}>
      <SessionLocationBridge location={navigation.current} />
      <SessionAttentionNotificationBridge />
      <TrayEventBridge
        onOpenAgent={handleTrayOpenAgent}
        onOpenProject={handleTrayOpenProject}
      />
      {section === "settings" ? (
        <SettingsPage
          activeTab={settingsTab}
          onBack={handleSettingsBack}
          onBackToProviderCatalog={() => navigation.replace({ section: "settings", tab: "model-provider" })}
          onOpenProvider={(target) =>
            navigation.navigate({
              section: "settings",
              tab: "model-provider",
              providerPresetKey: target.providerPresetKey ?? null,
              providerId: target.providerId ?? null,
            })
          }
          onSelectTab={(tab) => navigation.replace({ section: "settings", tab })}
          providerId={settingsLocation?.providerId ?? null}
          providerPresetKey={settingsLocation?.providerPresetKey ?? null}
        />
      ) : startupStatus === "ready" ? (
        <AppShellContent
          canBack={navigation.canBack}
          canForward={navigation.canForward}
          location={navigation.current}
          onBack={navigation.back}
          onForward={navigation.forward}
          onNavigate={navigation.navigate}
          onReplace={navigation.replace}
          section={section}
        />
      ) : (
        <StartupPage
          errorMessage={startupError}
          onConfigure={() => navigation.navigate({ section: "settings", tab: "model-provider" })}
          onRetry={() => void refreshStartupStatus()}
          status={startupStatus}
        />
      )}
    </SessionProvider>
  );
}

function SessionLocationBridge({ location }: { location: AppLocation }) {
  const { currentSessionId, draftMode, draftToOpen, openDraft, openSession } = useSession();

  useEffect(() => {
    if (location.section !== "session") return;
    if (location.draftId) {
      if (!draftMode || draftToOpen !== location.draftId || currentSessionId !== location.draftId) {
        openDraft(location.draftId);
      }
      return;
    }
    if ("sessionId" in location) {
      const targetId = location.sessionId ?? null;
      if (draftMode || currentSessionId !== targetId) {
        openSession(targetId);
      }
    }
  }, [currentSessionId, draftMode, draftToOpen, location, openDraft, openSession]);

  return null;
}

// 会话区：草稿态渲染独立草稿页（按草稿身份 key，切换草稿即重挂以触发保存）；否则渲染会话页。
function SessionArea() {
  const { draftMode, draftToOpen, newSessionRequestKey } = useSession();
  if (draftMode) {
    return <SessionDraftPage key={draftToOpen ?? `new-${newSessionRequestKey}`} />;
  }
  return <SessionPage />;
}

function AppShellContent({
  canBack,
  canForward,
  location,
  onBack,
  onForward,
  onNavigate,
  onReplace,
  section,
}: {
  canBack: boolean;
  canForward: boolean;
  location: AppLocation;
  onBack: () => void;
  onForward: () => void;
  onNavigate: (location: AppLocation) => void;
  onReplace: (location: AppLocation) => void;
  section: Exclude<AppSection, "settings">;
}) {
  const [sidebarMode, setSidebarMode] = useState<SidebarMode>("pinned");
  const [sessionSearchOpen, setSessionSearchOpen] = useState(false);
  const [appPlatform, setAppPlatform] = useState<AppPlatform>(() => detectInitialAppPlatform());
  const sessionChromeRef = useRef<HTMLElement | null>(null);
  const collapsedTitlebarActionsRef = useRef<HTMLDivElement | null>(null);
  const [measuredCollapsedContentInset, setMeasuredCollapsedContentInset] = useState<string | null>(
    null,
  );
  const { enterDraft } = useSession();
  const sidebarLayout = getSidebarLayoutState(sidebarMode, false);
  const titlebarLayout = getTitlebarLayoutState(appPlatform);
  const collapsedContentInset =
    measuredCollapsedContentInset ?? titlebarLayout.collapsedContentInsetFallback;
  const sessionChromeStyle: SessionChromeStyle = {
    "--session-body-padding-inline": sidebarMode === "collapsed" ? "40px" : "8px",
    "--session-header-padding-left": sidebarMode === "collapsed" ? collapsedContentInset : "1rem",
    "--titlebar-collapsed-actions-left": titlebarLayout.collapsedActionsLeft,
  };
  const pages: Record<Exclude<AppSection, "settings">, JSX.Element> = {
    home: (
      <HomePage
        onOpenAgent={(agentId) => onNavigate({ section: "agents", agentId })}
        onOpenAgents={() => onNavigate({ section: "agents" })}
        onOpenProject={(projectId) => onNavigate({ section: "projects", projectId })}
        onOpenProjects={() => onNavigate({ section: "projects" })}
        onOpenSettings={() => onNavigate({ section: "settings", tab: "model-provider" })}
      />
    ),
    session: <SessionArea />,
    extensions: (
      <ExtensionsPage
        tab={location.section === "extensions" ? location.tab : undefined}
        // 用 replace：Tab 切换不该塞满前进/后退历史。
        onSelectTab={(tab) => onReplace({ section: "extensions", tab })}
      />
    ),
    "knowledge-bases": (
      <KnowledgeBasesPage
        knowledgeBaseId={
          location.section === "knowledge-bases" ? location.knowledgeBaseId ?? null : null
        }
        onBack={() => {
          if (canBack) {
            onBack();
          } else {
            onReplace({ section: "knowledge-bases" });
          }
        }}
        onOpenKnowledgeBase={(id) => onNavigate({ section: "knowledge-bases", knowledgeBaseId: id })}
        onOpenList={() => onNavigate({ section: "knowledge-bases" })}
      />
    ),
    agents: (
      <AgentsPage
        agentId={location.section === "agents" ? location.agentId ?? null : null}
        onBack={() => {
          if (canBack) {
            onBack();
          } else {
            onReplace({ section: "agents" });
          }
        }}
        onOpenAgent={(agentId) => onNavigate({ section: "agents", agentId })}
        onOpenAgentList={() => onNavigate({ section: "agents" })}
        onOpenScheduledTask={(taskId) => onNavigate({ section: "scheduling", taskId })}
        onNewScheduledTask={(agentId) =>
          onNavigate({ section: "scheduling", agentId, create: true })
        }
      />
    ),
    projects: (
      <ProjectsPage
        onBack={() => {
          if (canBack) {
            onBack();
          } else {
            onReplace({ section: "projects" });
          }
        }}
        onOpenProject={(projectId) => onNavigate({ section: "projects", projectId })}
        onOpenProjectList={() => onNavigate({ section: "projects" })}
        onOpenScheduledTask={(taskId) => onNavigate({ section: "scheduling", taskId })}
        onNewScheduledTask={(projectId) =>
          onNavigate({ section: "scheduling", projectId, create: true })
        }
        projectId={location.section === "projects" ? location.projectId ?? null : null}
      />
    ),
    scheduling: (
      <ScheduledTasksPage
        agentId={location.section === "scheduling" ? location.agentId ?? null : null}
        create={location.section === "scheduling" ? Boolean(location.create) : false}
        onBack={() => {
          if (canBack) {
            onBack();
          } else {
            onReplace({ section: "scheduling" });
          }
        }}
        onOpenTask={(taskId) => onNavigate({ section: "scheduling", taskId })}
        onReplace={(nextLocation) => onReplace(nextLocation)}
        projectId={location.section === "scheduling" ? location.projectId ?? null : null}
        taskId={location.section === "scheduling" ? location.taskId ?? null : null}
      />
    ),
    remote: <RemotePage onOpenSession={(sessionId) => onNavigate({ section: "session", sessionId })} />,
  };

  useEffect(() => {
    const devOverride = getDevAppPlatformOverride();
    if (devOverride) {
      setAppPlatform(devOverride);
      return;
    }
    getAppPlatform()
      .then(setAppPlatform)
      .catch(() => {
        // Browser preview has no Tauri IPC; keep the navigator-derived fallback.
      });
  }, []);

  useLayoutEffect(() => {
    if (sidebarMode !== "collapsed") {
      setMeasuredCollapsedContentInset(null);
      return;
    }

    const actionsElement = collapsedTitlebarActionsRef.current;
    if (!actionsElement) {
      setMeasuredCollapsedContentInset(null);
      return;
    }

    const updateCollapsedContentInset = () => {
      const chromeLeft = sessionChromeRef.current?.getBoundingClientRect().left ?? 0;
      const rightEdge = actionsElement.getBoundingClientRect().right - chromeLeft;
      const nextInset = `${Math.ceil(rightEdge + COLLAPSED_CONTENT_INSET_GAP_PX)}px`;
      setMeasuredCollapsedContentInset((current) => (current === nextInset ? current : nextInset));
    };

    updateCollapsedContentInset();
    window.addEventListener("resize", updateCollapsedContentInset);

    if (typeof ResizeObserver === "undefined") {
      return () => {
        window.removeEventListener("resize", updateCollapsedContentInset);
      };
    }

    const observer = new ResizeObserver(updateCollapsedContentInset);
    observer.observe(actionsElement);
    if (sessionChromeRef.current) {
      observer.observe(sessionChromeRef.current);
    }

    return () => {
      observer.disconnect();
      window.removeEventListener("resize", updateCollapsedContentInset);
    };
  }, [sidebarMode, titlebarLayout.collapsedActionsLeft]);

  function toggleSidebarMode() {
    setSidebarMode((current) => (current === "pinned" ? "collapsed" : "pinned"));
  }

  function handleOpenProject(projectId: string) {
    onNavigate({ section: "projects", projectId });
  }

  function handleOpenProjectList() {
    onNavigate({ section: "projects" });
  }

  function handleOpenAgent(agentId: string) {
    onNavigate({ section: "agents", agentId });
  }

  function handleOpenAgentList() {
    onNavigate({ section: "agents" });
  }

  return (
    <main
      className="grid h-screen bg-transparent text-foreground transition-[grid-template-columns] duration-150"
      style={{ gridTemplateColumns: sidebarLayout.gridColumns }}
    >
      <WindowDragRegion className="h-5 w-full" />

      <div className="relative h-screen overflow-hidden bg-transparent">
        <div
          aria-hidden={!sidebarLayout.sidebarVisible}
          className={`absolute inset-y-0 left-0 z-30 transition-[opacity,transform] duration-150 ${
            sidebarLayout.sidebarVisible
              ? "visible translate-x-0 opacity-100"
              : "invisible pointer-events-none -translate-x-2 opacity-0"
          }`}
          style={{ width: SIDEBAR_WIDTH_PX }}
        >
          <Sidebar
            activeSection={section}
            canBack={canBack}
            canForward={canForward}
            mode={sidebarMode}
            onBack={onBack}
            onCreateProject={handleOpenProject}
            onForward={onForward}
            onNavigateDraft={(draftId) => onNavigate({ section: "session", draftId })}
            onNavigateSession={(sessionId) => onNavigate({ section: "session", sessionId })}
            onOpenAgent={handleOpenAgent}
            onOpenAgentList={handleOpenAgentList}
            onOpenProject={handleOpenProject}
            onOpenProjectList={handleOpenProjectList}
            onOpenRemoteConfig={() => onNavigate({ section: "remote" })}
            onSearch={() => setSessionSearchOpen(true)}
            onSelectSection={(nextSection) => {
              if (nextSection === "settings") {
                onNavigate({ section: "settings", tab: "model-provider" });
              } else {
                onNavigate({ section: nextSection });
              }
            }}
            onToggleMode={toggleSidebarMode}
            platform={appPlatform}
          />
        </div>
      </div>
      <section
        ref={sessionChromeRef}
        className="relative flex min-w-0 overflow-auto flex-col bg-background [&_.session-body]:px-[var(--session-body-padding-inline)] [&_.session-body]:transition-[padding] [&_.session-body]:duration-150 [&_.session-header]:pl-[var(--session-header-padding-left)]"
        style={sessionChromeStyle}
      >
        {sidebarMode === "collapsed" && (
          <SidebarTitlebarActions
            ref={collapsedTitlebarActionsRef}
            canBack={canBack}
            canForward={canForward}
            className={COLLAPSED_SIDEBAR_TITLEBAR_ACTIONS_CLASS_NAME}
            homeActive={section === "home"}
            mode={sidebarMode}
            onBack={onBack}
            onForward={onForward}
            onHome={() => onNavigate({ section: "home" })}
            onNewTask={enterDraft}
            onSearch={() => setSessionSearchOpen(true)}
            onToggleMode={() => setSidebarMode("pinned")}
          />
        )}
        <div className="min-h-0 flex-1">{pages[section]}</div>
        <SessionSearchModal
          open={sessionSearchOpen}
          onClose={() => setSessionSearchOpen(false)}
        />
      </section>
    </main>
  );
}

export default function App() {
  return (
    <div className="app-theme relative h-screen">
      <WindowDragRegion className="fixed inset-x-0 top-0 z-[15] h-4" />
      <NotificationProvider>
        <MessageProvider>
          <AppShell />
        </MessageProvider>
      </NotificationProvider>
    </div>
  );
}
