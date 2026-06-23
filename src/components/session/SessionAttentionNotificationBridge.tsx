import { useEffect, useRef } from "react";

import { subscribeAgentStreamEvents } from "../../api";
import type { AgentStreamEvent } from "../../types";
import { showSystemNotification } from "../../lib/systemNotifications";
import { useSession } from "./SessionProvider";

export function SessionAttentionNotificationBridge() {
  const { currentSessionId, openSession } = useSession();
  const currentSessionIdRef = useRef<string | null>(currentSessionId);
  const notifiedKeysRef = useRef<Set<string>>(new Set());
  currentSessionIdRef.current = currentSessionId;

  useEffect(() => {
    let un: (() => void) | undefined;
    let cancelled = false;

    subscribeAgentStreamEvents((e: AgentStreamEvent) => {
      if (!isSessionAttentionEvent(e)) return;

      const targetSessionId = e.sessionId;
      if (!targetSessionId || targetSessionId === currentSessionIdRef.current) return;

      const key = `${e.kind}:${targetSessionId}:${e.toolCallId ?? e.messageId}`;
      if (notifiedKeysRef.current.has(key)) return;
      notifiedKeysRef.current.add(key);

      const notification = buildSessionAttentionNotification(e);
      void showSystemNotification({
        ...notification,
        tag: key,
        onClick: () => openSession(targetSessionId),
      });
    }).then((fn) => {
      if (cancelled) fn();
      else un = fn;
    });

    return () => {
      cancelled = true;
      un?.();
    };
  }, [openSession]);

  return null;
}

function isSessionAttentionEvent(e: AgentStreamEvent): boolean {
  return e.kind === "permission_required" || e.kind === "ask_required";
}

export function buildSessionAttentionNotification(e: AgentStreamEvent): {
  body: string;
  title: string;
} {
  const actor = e.expertName?.trim() ? `专家「${e.expertName.trim()}」` : "会话";
  if (e.kind === "permission_required") {
    return {
      title: "需要权限确认",
      body: `${actor} 请求执行需要确认的操作。`,
    };
  }
  return {
    title: "需要你回答",
    body: `${actor} 提出了一个问题。`,
  };
}
