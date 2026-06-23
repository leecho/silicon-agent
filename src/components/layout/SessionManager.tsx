import { useEffect, useState } from "react";
import {
  createSessionGroup,
  deleteSession,
  deleteSessionGroup,
  renameSession,
  setSessionGroup,
  setSessionPinned,
  updateSessionGroup,
} from "../../api";
import { useMessages } from "../../components/ui";
import { useSession } from "../session/SessionProvider";
import type { SessionGroup, SessionInfo } from "../../types";
import { GroupFormModal } from "./session-manager/GroupFormModal";
import { NormalSessions } from "./session-manager/NormalSessions";
import { RemoteSessions } from "./session-manager/RemoteSessions";
import { SessionActionMenu } from "./session-manager/SessionActionMenu";
import { type GroupForm, remoteChannels } from "./session-manager/sessionManagerShared";
import { useSessionManagerData } from "./session-manager/useSessionManagerData";

// 会话管理（收进主 Sidebar 中部）：项目、智能体、普通会话/草稿和远程连接入口的组合层。
// 数据加载和订阅集中在 useSessionManagerData；各形态 session 的节点投影在对应组件内。
export function SessionManager({
  onNavigateDraft,
  onNavigateSession,
}: {
  onNavigateDraft: (draftId: string) => void;
  onNavigateSession: (sessionId: string) => void;
}) {
  const messages = useMessages();
  const {
    currentSessionId,
    openSession,
    sessionRefreshKey,
  } = useSession();
  const {
    groups,
    notifyError,
    refreshAll,
    remoteBindings,
    sessions,
  } = useSessionManagerData({
    currentSessionId,
    sessionRefreshKey,
  });

  const [menuSessionId, setMenuSessionId] = useState<string | null>(null);
  const [menuPosition, setMenuPosition] = useState({ x: 138, y: 220 });
  const [busySessionId, setBusySessionId] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [remoteExpanded, setRemoteExpanded] = useState<Set<string>>(
    () => new Set(["__remote_section__", ...remoteChannels.map((channel) => channel.id)]),
  );
  // 分组表单：create=新建（创建后移入会话）；edit=编辑现有分组。null=关闭。
  const [groupForm, setGroupForm] = useState<GroupForm | null>(null);
  const [groupName, setGroupName] = useState("");
  const [groupColor, setGroupColor] = useState("#0090FF");

  const menuSession = sessions.find((session) => session.id === menuSessionId) ?? null;
  const normalMenuSession = menuSession;
  const userSessionCount = sessions.filter(
    (s) => !s.origin || s.origin === "user",
  ).length;
  const remoteChannelIds = new Set(remoteChannels.map((channel) => channel.id));
  const remoteSessionIds = new Set(
    remoteBindings
      .filter((binding) => remoteChannelIds.has(binding.channel as typeof remoteChannels[number]["id"]))
      .map((binding) => binding.sessionId),
  );
  const remoteSessionCount = sessions.filter(
    (s) => s.origin === "remote" && remoteSessionIds.has(s.id),
  ).length;
  const isEmpty = userSessionCount === 0 && remoteSessionCount === 0;

  function openSessionMenu(sessionId: string, x: number, y: number) {
    setMenuSessionId(sessionId);
    setMenuPosition({
      x: Math.max(8, Math.min(x, window.innerWidth - 196)),
      y: Math.max(8, Math.min(y, window.innerHeight - 200)),
    });
  }

  function toggleRemoteChannel(channelId: string) {
    setRemoteExpanded((current) => {
      const next = new Set(current);
      if (next.has(channelId)) {
        next.delete(channelId);
      } else {
        next.add(channelId);
      }
      return next;
    });
  }

  function toggleCollapsed(key: string) {
    setCollapsed((current) => ({ ...current, [key]: !current[key] }));
  }

  // 点击任意处 / Esc 关闭更多菜单。
  useEffect(() => {
    function closeMenu() {
      setMenuSessionId(null);
    }
    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") closeMenu();
    }
    document.addEventListener("click", closeMenu);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("click", closeMenu);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, []);

  async function handleRename(session: SessionInfo) {
    setMenuSessionId(null);
    const name = (
      await messages.prompt({
        title: "重命名会话",
        message: "输入新名称",
        defaultValue: session.title,
        placeholder: "会话标题",
        confirmText: "保存",
      })
    )?.trim();
    if (!name || name === session.title) return;
    try {
      await renameSession(session.id, name);
      await refreshAll();
    } catch (err) {
      notifyError("重命名失败", err);
    }
  }

  async function handleDelete(session: SessionInfo) {
    setMenuSessionId(null);
    const ok = await messages.confirm({
      title: "删除会话",
      message: "确定删除？此操作不可撤销。",
      tone: "warning",
      confirmText: "删除",
    });
    if (!ok) return;
    setBusySessionId(session.id);
    try {
      await deleteSession(session.id);
      const list = await refreshAll();
      if (session.id === currentSessionId) {
        const next = list.find((candidate) => candidate.id !== session.id) ?? null;
        if (next) {
          onNavigateSession(next.id);
        } else {
          openSession(null);
        }
      }
    } catch (err) {
      notifyError("删除失败", err);
    } finally {
      setBusySessionId(null);
    }
  }

  async function handleTogglePinned(session: SessionInfo) {
    setMenuSessionId(null);
    setBusySessionId(session.id);
    try {
      await setSessionPinned(session.id, !session.pinned);
      await refreshAll();
    } catch (err) {
      notifyError(session.pinned ? "取消置顶失败" : "置顶失败", err);
    } finally {
      setBusySessionId(null);
    }
  }

  async function handleMoveToGroup(session: SessionInfo, groupId: string | null) {
    setMenuSessionId(null);
    setBusySessionId(session.id);
    try {
      await setSessionGroup(session.id, groupId);
      await refreshAll();
    } catch (err) {
      notifyError(groupId ? "移入分组失败" : "移出分组失败", err);
    } finally {
      setBusySessionId(null);
    }
  }

  // 打开「新建分组」表单（名称 + 取色器）；创建后把该会话移入新分组。
  function handleNewGroup(session: SessionInfo) {
    setMenuSessionId(null);
    setGroupName("新分组");
    setGroupColor("#0090FF");
    setGroupForm({ mode: "create", session });
  }

  // 打开「编辑分组」表单（仅用户分组）。
  function handleEditGroup(group: SessionGroup) {
    setGroupName(group.label);
    setGroupColor(group.colorKey.startsWith("#") ? group.colorKey : "#0090FF");
    setGroupForm({ mode: "edit", group });
  }

  async function confirmGroupForm() {
    const form = groupForm;
    const name = groupName.trim();
    if (!form || !name) return;
    setGroupForm(null);
    try {
      if (form.mode === "create") {
        const g = await createSessionGroup(name, groupColor);
        await setSessionGroup(form.session.id, g.id);
      } else {
        await updateSessionGroup(form.group.id, name, groupColor);
      }
      await refreshAll();
    } catch (err) {
      notifyError(form.mode === "create" ? "新建分组失败" : "编辑分组失败", err);
    }
  }

  async function handleDeleteGroup(group: SessionGroup) {
    const ok = await messages.confirm({
      title: "删除分组",
      message: "删除后其会话归入最近。",
      tone: "warning",
      confirmText: "删除",
    });
    if (!ok) return;
    try {
      await deleteSessionGroup(group.id);
      await refreshAll();
    } catch (err) {
      notifyError("删除分组失败", err);
    }
  }

  return (
    <>
      <div className="flex min-h-0 flex-1 flex-col gap-6">
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto">
          <NormalSessions
            busySessionId={busySessionId}
            collapsed={collapsed}
            currentSessionId={currentSessionId}
            emptyLabel={isEmpty ? "暂无会话" : undefined}
            groups={groups}
            onDeleteGroup={(group) => void handleDeleteGroup(group)}
            onEditGroup={handleEditGroup}
            onOpenDraft={onNavigateDraft}
            onOpenSession={onNavigateSession}
            onOpenSessionMenu={openSessionMenu}
            onToggleCollapsed={toggleCollapsed}
            sessions={sessions}
          />
          <RemoteSessions
            currentSessionId={currentSessionId}
            onOpenSession={onNavigateSession}
            onToggleChannel={toggleRemoteChannel}
            remoteBindings={remoteBindings}
            remoteExpanded={remoteExpanded}
            sessions={sessions}
          />
        </div>
      </div>

      {normalMenuSession && (
        <SessionActionMenu
          groups={groups}
          menuPosition={menuPosition}
          menuSession={normalMenuSession}
          onDelete={(session) => void handleDelete(session)}
          onMoveToGroup={(session, groupId) => void handleMoveToGroup(session, groupId)}
          onNewGroup={handleNewGroup}
          onRename={(session) => void handleRename(session)}
          onTogglePinned={(session) => void handleTogglePinned(session)}
        />
      )}

      <GroupFormModal
        groupColor={groupColor}
        groupForm={groupForm}
        groupName={groupName}
        onClose={() => setGroupForm(null)}
        onConfirm={() => void confirmGroupForm()}
        onGroupColorChange={setGroupColor}
        onGroupNameChange={setGroupName}
      />

    </>
  );
}
