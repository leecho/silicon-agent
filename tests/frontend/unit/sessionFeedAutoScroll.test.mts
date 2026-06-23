import { readFileSync } from "node:fs";

const source = readFileSync("src/pages/session/SessionPage.tsx", "utf8");

if (!source.includes("useLayoutEffect")) {
  throw new Error("Session feed auto-scroll should run in a layout effect before paint");
}

if (!source.includes("feedScrollRef")) {
  throw new Error("Session feed scroll container should be owned through an explicit ref");
}

if (!source.includes("const [feedVersion, bump] = useState(0)")) {
  throw new Error("Session feed should expose a version state for scroll effects after feed mutations");
}

if (!source.includes("feedScrollRef.current")) {
  throw new Error("Session feed auto-scroll should read the scroll container from feedScrollRef");
}

if (!source.includes("SESSION_FEED_STICKY_BOTTOM_PX")) {
  throw new Error("Session feed auto-scroll should use a bottom threshold for sticky scrolling");
}

if (!source.includes("feedPinnedToBottomRef")) {
  throw new Error("Session feed should remember whether the user is currently pinned to the bottom");
}

if (!source.includes("lastAutoScrolledSessionIdRef")) {
  throw new Error("Session feed should track session changes separately from feed updates");
}

if (!source.includes("function isFeedNearBottom")) {
  throw new Error("Session feed should compute whether the scroll container is near the bottom");
}

if (!source.includes("const sessionChanged = lastAutoScrolledSessionIdRef.current !== detail.session.id")) {
  throw new Error("Session feed should force-scroll only when opening or switching sessions");
}

if (!source.includes("if (!sessionChanged && !feedPinnedToBottomRef.current) return")) {
  throw new Error("Session feed updates should not pull the user down after they manually scroll up");
}

if (!source.includes("scrollTop = el.scrollHeight")) {
  throw new Error("Session feed should still scroll to the bottom when sticky scrolling is active");
}

if (!source.includes("ref={feedScrollRef}")) {
  throw new Error("Session body scroll container should attach feedScrollRef");
}

if (!source.includes("onScroll={handleFeedScroll}")) {
  throw new Error("Session body scroll container should update sticky state on user scroll");
}

if (!source.includes("[detail?.session.id, feedVersion")) {
  throw new Error("Session feed auto-scroll should run when opening a session and when feed updates");
}
