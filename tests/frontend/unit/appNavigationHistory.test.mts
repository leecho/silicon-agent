import {
  appNavigationReducer,
  initialAppNavigationState,
  locationsEqual,
  type AppNavigationState,
} from "../../../src/hooks/useAppNavigation.ts";

function assertEqual<T>(actual: T, expected: T, message: string) {
  if (actual !== expected) {
    throw new Error(`${message}: expected ${String(expected)}, got ${String(actual)}`);
  }
}

function assertState(
  actual: AppNavigationState,
  expected: AppNavigationState,
  message: string,
) {
  const actualJson = JSON.stringify(actual);
  const expectedJson = JSON.stringify(expected);
  if (actualJson !== expectedJson) {
    throw new Error(`${message}: expected ${expectedJson}, got ${actualJson}`);
  }
}

let state = appNavigationReducer(initialAppNavigationState, {
  type: "navigate",
  location: { section: "projects" },
});

assertState(
  state,
  {
    backStack: [{ section: "home" }],
    current: { section: "projects" },
    forwardStack: [],
  },
  "navigate should push current location and clear forward history",
);

state = appNavigationReducer(state, {
  type: "navigate",
  location: { section: "projects" },
});

assertState(
  state,
  {
    backStack: [{ section: "home" }],
    current: { section: "projects" },
    forwardStack: [],
  },
  "duplicate navigation should not add history entries",
);

state = appNavigationReducer(state, {
  type: "navigate",
  location: { section: "settings", tab: "preferences" },
});

state = appNavigationReducer(state, { type: "back" });

assertState(
  state,
  {
    backStack: [{ section: "home" }],
    current: { section: "projects" },
    forwardStack: [{ section: "settings", tab: "preferences" }],
  },
  "back should restore the previous location and keep forward history",
);

state = appNavigationReducer(state, { type: "forward" });

assertState(
  state,
  {
    backStack: [{ section: "home" }, { section: "projects" }],
    current: { section: "settings", tab: "preferences" },
    forwardStack: [],
  },
  "forward should restore the next location and keep back history",
);

state = appNavigationReducer(state, {
  type: "replace",
  location: { section: "session", sessionId: "session-1" },
});

assertState(
  state,
  {
    backStack: [{ section: "home" }, { section: "projects" }],
    current: { section: "session", sessionId: "session-1" },
    forwardStack: [],
  },
  "replace should update current without changing history stacks",
);

assertEqual(
  locationsEqual({ section: "projects", projectId: "project-1" }, { section: "projects", projectId: "project-1" }),
  true,
  "matching project detail locations should be equal",
);

assertEqual(
  locationsEqual({ section: "projects", projectId: "project-1" }, { section: "projects", projectId: "project-2" }),
  false,
  "different project detail locations should not be equal",
);

assertEqual(
  locationsEqual({ section: "settings", tab: "memory" }, { section: "settings", tab: "memory" }),
  true,
  "matching settings tab locations should be equal",
);

assertEqual(
  locationsEqual(
    { section: "settings", tab: "model-provider", providerPresetKey: "deepseek" },
    { section: "settings", tab: "model-provider", providerPresetKey: "deepseek" },
  ),
  true,
  "matching settings provider preset detail locations should be equal",
);

assertEqual(
  locationsEqual(
    { section: "settings", tab: "model-provider", providerPresetKey: "deepseek" },
    { section: "settings", tab: "model-provider", providerPresetKey: "openai" },
  ),
  false,
  "different settings provider preset detail locations should not be equal",
);

assertEqual(
  locationsEqual({ section: "session", sessionId: "session-1" }, { section: "session", draftId: "session-1" }),
  false,
  "session and draft locations should not be equal",
);
