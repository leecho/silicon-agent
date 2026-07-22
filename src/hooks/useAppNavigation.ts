import { useCallback, useMemo, useReducer } from "react";
import type { AppSection } from "../appNavigation";
import type { ExtensionTabId } from "../pages/extensions/extensionTabs";
import type { SettingsTabId } from "../pages/settings/settingsTabs";

export type AppLocation =
  | { section: "home" }
  | { section: "session"; sessionId?: string | null; draftId?: string | null }
  | { section: "agents"; agentId?: string | null }
  | { section: "knowledge-bases"; knowledgeBaseId?: string | null }
  | { section: "projects"; projectId?: string | null }
  | { section: "extensions"; tab?: ExtensionTabId }
  | {
      section: "scheduling";
      taskId?: string | null;
      projectId?: string | null;
      agentId?: string | null;
      create?: boolean;
    }
  | {
      section: "settings";
      tab?: SettingsTabId;
      providerPresetKey?: string | null;
      providerId?: string | null;
    }
  | {
      section: Exclude<
        AppSection,
        | "session"
        | "agents"
        | "knowledge-bases"
        | "projects"
        | "scheduling"
        | "settings"
        | "extensions"
      >;
    };

export interface AppNavigationState {
  backStack: AppLocation[];
  current: AppLocation;
  forwardStack: AppLocation[];
}

export type AppNavigationAction =
  | { type: "navigate"; location: AppLocation }
  | { type: "replace"; location: AppLocation }
  | { type: "back" }
  | { type: "forward" };

export const initialAppNavigationState: AppNavigationState = {
  backStack: [],
  current: { section: "home" },
  forwardStack: [],
};

export function locationsEqual(left: AppLocation, right: AppLocation): boolean {
  if (left.section !== right.section) return false;
  switch (left.section) {
    case "session":
      return (
        (left.sessionId ?? null) ===
          ((right as Extract<AppLocation, { section: "session" }>).sessionId ?? null) &&
        (left.draftId ?? null) ===
          ((right as Extract<AppLocation, { section: "session" }>).draftId ?? null)
      );
    case "agents":
      return (
        (left.agentId ?? null) ===
        ((right as Extract<AppLocation, { section: "agents" }>).agentId ?? null)
      );
    case "knowledge-bases":
      return (
        (left.knowledgeBaseId ?? null) ===
        ((right as Extract<AppLocation, { section: "knowledge-bases" }>).knowledgeBaseId ?? null)
      );
    case "projects":
      return (
        (left.projectId ?? null) ===
        ((right as Extract<AppLocation, { section: "projects" }>).projectId ?? null)
      );
    case "scheduling": {
      const rightScheduling = right as Extract<AppLocation, { section: "scheduling" }>;
      return (
        (left.taskId ?? null) === (rightScheduling.taskId ?? null) &&
        (left.projectId ?? null) === (rightScheduling.projectId ?? null) &&
        (left.agentId ?? null) === (rightScheduling.agentId ?? null) &&
        Boolean(left.create) === Boolean(rightScheduling.create)
      );
    }
    // 必须比 tab：否则「同 section 即同位置」会让 replace 判定无变化而丢弃切 Tab
    // （表现为扩展页永远停在落地的「市场」Tab）。
    case "extensions":
      return (
        (left.tab ?? null) ===
        ((right as Extract<AppLocation, { section: "extensions" }>).tab ?? null)
      );
    case "settings":
      return (
        (left.tab ?? null) ===
          ((right as Extract<AppLocation, { section: "settings" }>).tab ?? null) &&
        (left.providerPresetKey ?? null) ===
          ((right as Extract<AppLocation, { section: "settings" }>).providerPresetKey ?? null) &&
        (left.providerId ?? null) ===
          ((right as Extract<AppLocation, { section: "settings" }>).providerId ?? null)
      );
    default:
      return true;
  }
}

export function appNavigationReducer(
  state: AppNavigationState,
  action: AppNavigationAction,
): AppNavigationState {
  switch (action.type) {
    case "navigate":
      if (locationsEqual(state.current, action.location)) return state;
      return {
        backStack: [...state.backStack, state.current],
        current: action.location,
        forwardStack: [],
      };
    case "replace":
      if (locationsEqual(state.current, action.location)) return state;
      return {
        ...state,
        current: action.location,
      };
    case "back": {
      const previous = state.backStack[state.backStack.length - 1];
      if (!previous) return state;
      return {
        backStack: state.backStack.slice(0, -1),
        current: previous,
        forwardStack: [state.current, ...state.forwardStack],
      };
    }
    case "forward": {
      const next = state.forwardStack[0];
      if (!next) return state;
      return {
        backStack: [...state.backStack, state.current],
        current: next,
        forwardStack: state.forwardStack.slice(1),
      };
    }
    default:
      return state;
  }
}

export function useAppNavigation(initialLocation: AppLocation = { section: "home" }) {
  const [state, dispatch] = useReducer(appNavigationReducer, {
    ...initialAppNavigationState,
    current: initialLocation,
  });

  const navigate = useCallback((location: AppLocation) => {
    dispatch({ type: "navigate", location });
  }, []);

  const replace = useCallback((location: AppLocation) => {
    dispatch({ type: "replace", location });
  }, []);

  const back = useCallback(() => {
    dispatch({ type: "back" });
  }, []);

  const forward = useCallback(() => {
    dispatch({ type: "forward" });
  }, []);

  return useMemo(
    () => ({
      ...state,
      navigate,
      replace,
      back,
      forward,
      canBack: state.backStack.length > 0,
      canForward: state.forwardStack.length > 0,
    }),
    [back, forward, navigate, replace, state],
  );
}
