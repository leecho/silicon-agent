import { useCallback, useEffect, useRef, useState } from "react";
import {
  permissionOpenSettings,
  permissionRequest,
  permissionStatusAll,
  type PermissionKind,
  type PermissionRow,
} from "../api";

type LoadState = "loading" | "ready" | "error";

export function useSystemPermissions() {
  const [rows, setRows] = useState<PermissionRow[]>([]);
  const [loadState, setLoadState] = useState<LoadState>("loading");
  const aliveRef = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const next = await permissionStatusAll();
      if (aliveRef.current) {
        setRows(next);
        setLoadState("ready");
      }
    } catch {
      if (aliveRef.current) setLoadState("error");
    }
  }, []);

  useEffect(() => {
    aliveRef.current = true;
    void refresh();
    // 用户在系统设置里外部授权（尤其无法编程唤起的 FDA）后，回到 app 必须重检。
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => {
      aliveRef.current = false;
      window.removeEventListener("focus", onFocus);
    };
  }, [refresh]);

  const authorize = useCallback(
    async (kind: PermissionKind, canRequest: boolean) => {
      if (canRequest) {
        await permissionRequest(kind).catch(() => {});
      }
      // 不论能否编程唤起，都跳设置兜底（弹窗被系统抑制 / 不可编程时的唯一入口）。
      await permissionOpenSettings(kind).catch(() => {});
      await refresh();
    },
    [refresh],
  );

  return { rows, loadState, refresh, authorize };
}
