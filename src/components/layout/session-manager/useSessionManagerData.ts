import { useEffect, useState } from "react";
import {
  cleanupEmptyDrafts,
  getSession,
  listSessions,
  listSessionGroups,
  subscribeAgentStreamEvents,
  subscribeSessionUpdated,
} from "../../../api";
import { listRemoteBindings, type RemoteBinding } from "../../../api/remote";
import { useNotifications } from "../../../components/ui";
import type { AgentStreamEvent, SessionGroup, SessionInfo } from "../../../types";

function isTopLevelSession(session: SessionInfo): boolean {
  return !session.parentSessionId;
}

export function useSessionManagerData({
  currentSessionId,
  sessionRefreshKey,
}: {
  currentSessionId: string | null;
  sessionRefreshKey: number;
}) {
  const notifications = useNotifications();
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [groups, setGroups] = useState<SessionGroup[]>([]);
  const [remoteBindings, setRemoteBindings] = useState<RemoteBinding[]>([]);

  function notifyError(title: string, err: unknown) {
    notifications.notify({
      tone: "error",
      title,
      message: err instanceof Error ? err.message : String(err),
    });
  }

  async function refreshSessions(): Promise<SessionInfo[]> {
    const list = (await listSessions()).filter(isTopLevelSession);
    setSessions(list);
    return list;
  }

  async function refreshGroups(): Promise<void> {
    try {
      const gs = await listSessionGroups();
      setGroups(gs);
    } catch (err) {
      notifyError("加载分组失败", err);
    }
  }

  async function refreshRemoteBindings(): Promise<void> {
    try {
      const bindings = await listRemoteBindings();
      setRemoteBindings(bindings);
    } catch (err) {
      notifyError("加载远程会话失败", err);
    }
  }

  async function refreshAll(): Promise<SessionInfo[]> {
    const list = await refreshSessions();
    await Promise.all([refreshGroups(), refreshRemoteBindings()]);
    return list;
  }

  function setSessionRunning(sessionId: string, isRunning: boolean) {
    setSessions((current) =>
      current.map((session) =>
        session.id === sessionId ? { ...session, isRunning } : session,
      ),
    );
  }

  // 初次：清理空草稿 -> 拉会话列表和分组。是否打开会话由当前页面/用户动作决定，
  // 避免侧边栏加载时把首页强制跳到会话页。
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        await cleanupEmptyDrafts().catch(() => {});
        await refreshAll();
        if (cancelled) return;
      } catch (err) {
        if (!cancelled) notifyError("加载会话失败", err);
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 会话内容活动（标题、updated_at 等）由页面发信号，列表刷新留在 SessionManager 内部。
  useEffect(() => {
    if (sessionRefreshKey === 0) return;
    let cancelled = false;
    refreshAll()
      .then(async (list) => {
        if (cancelled) return;
        if (
          currentSessionId &&
          !list.some((session) => session.id === currentSessionId)
        ) {
          // 列表只含顶层会话；子会话（如打开的专家子会话）本就不在其中，不能据此判定「不存在」。
          // 真正查不到该会话时才提示，避免在子会话里批准权限等操作后误报。
          const stillExists = await getSession(currentSessionId)
            .then((s) => s !== null)
            .catch(() => true);
          if (!cancelled && !stillExists) {
            notifyError("当前会话已不存在", "请从列表重新选择一个会话。");
          }
        }
      })
      .catch((err) => {
        if (!cancelled) notifyError("刷新会话失败", err);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionRefreshKey]);

  // 后台标题生成等会话元信息更新 -> 刷新列表（拿到新标题）。
  useEffect(() => {
    let un: (() => void) | undefined;
    let cancelled = false;
    subscribeSessionUpdated(() => {
      void refreshAll();
    }).then((u) => {
      if (cancelled) u();
      else un = u;
    });
    return () => {
      cancelled = true;
      un?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // run 生命周期不一定会改变标题/分组，但需要即时同步列表行的运行态。
  useEffect(() => {
    let un: (() => void) | undefined;
    let cancelled = false;
    subscribeAgentStreamEvents((event: AgentStreamEvent) => {
      if (event.kind === "run_started") {
        setSessionRunning(event.sessionId, true);
        return;
      }
      if (event.kind === "run_finished") {
        setSessionRunning(event.sessionId, false);
        void refreshSessions().catch((err) => notifyError("刷新会话运行状态失败", err));
      }
    }).then((u) => {
      if (cancelled) u();
      else un = u;
    });
    return () => {
      cancelled = true;
      un?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return {
    groups,
    notifyError,
    refreshAll,
    refreshSessions,
    remoteBindings,
    sessions,
  };
}
