import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { type McpServerStatus, mcpServerStatuses } from "../../api";

/**
 * 有多少个服务在等用户登录。
 *
 * 用来给「扩展 → MCP」Tab 打待办角标：装完一个带 OAuth 的插件（如 Figma）后，
 * 用户没有任何理由知道自己还差「去 MCP 页点一次登录」这一步——不主动找上门，
 * 他只会觉得「装了但不好使」。
 */
export function useMcpNeedsLogin(): number {
  const [count, setCount] = useState(0);

  useEffect(() => {
    let alive = true;
    const statuses = new Map<string, McpServerStatus["state"]>();

    function recount() {
      if (!alive) return;
      let n = 0;
      for (const state of statuses.values()) if (state === "unauthorized") n += 1;
      setCount(n);
    }

    void mcpServerStatuses().then((all) => {
      for (const s of all) statuses.set(s.serverId, s.state);
      recount();
    });

    const unlisten = listen<McpServerStatus>("mcp_status_event", (e) => {
      statuses.set(e.payload.serverId, e.payload.state);
      recount();
    });

    return () => {
      alive = false;
      void unlisten.then((f) => f());
    };
  }, []);

  return count;
}
