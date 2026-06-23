import { useEffect } from "react";
import {
  subscribeTrayNewTask,
  subscribeTrayOpenAgent,
  subscribeTrayOpenProject,
  subscribeTrayOpenSession,
  refreshTrayMenu,
  subscribeSessionUpdated,
} from "../../api";
import { useSession } from "../session/SessionProvider";

export function TrayEventBridge({
  onOpenAgent,
  onOpenProject,
}: {
  onOpenAgent: (agentId: string) => void;
  onOpenProject: (projectId: string) => void;
}) {
  const { enterDraft, openSession } = useSession();

  useEffect(() => {
    let disposed = false;
    const unlisten: Array<() => void> = [];

    const register = async () => {
      const next = await Promise.all([
        subscribeTrayNewTask(() => enterDraft()),
        subscribeTrayOpenProject(({ id }) => onOpenProject(id)),
        subscribeTrayOpenAgent(({ id }) => onOpenAgent(id)),
        subscribeTrayOpenSession(({ id }) => openSession(id)),
        subscribeSessionUpdated(() => {
          void refreshTrayMenu().catch((err) => {
            console.debug("refresh tray menu failed", err);
          });
        }),
      ]);
      if (disposed) {
        next.forEach((unsubscribe) => unsubscribe());
        return;
      }
      unlisten.push(...next);
    };

    void register();

    return () => {
      disposed = true;
      unlisten.forEach((unsubscribe) => unsubscribe());
    };
  }, [enterDraft, onOpenAgent, onOpenProject, openSession]);

  return null;
}
