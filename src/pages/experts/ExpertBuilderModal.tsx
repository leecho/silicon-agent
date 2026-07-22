import { useEffect, useState } from "react";
import { createExpert } from "../../api";
import { Button } from "../../components/ui/Button";
import { Modal, ModalHeader } from "../../components/ui/Modal";
import { useNotifications } from "../../components/ui/NotificationProvider";
import type { ExpertSummary } from "../../types";

/** 新建散装 agent 表单：身份 + system prompt 正文。
 * 模型档位与工具白名单不向用户暴露——用默认（aux 模型；工具留空，运行期回退通用工具集）。逻辑保留于后端。 */
export function ExpertBuilderModal({
  open,
  onClose,
  onCreated,
}: {
  open: boolean;
  onClose: () => void;
  onCreated: (agent: ExpertSummary) => void;
}) {
  const notifications = useNotifications();
  const [name, setName] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [profession, setProfession] = useState("");
  const [description, setDescription] = useState("");
  const [systemPrompt, setSystemPrompt] = useState("");
  const [quickPromptsText, setQuickPromptsText] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) return;
    setName("");
    setDisplayName("");
    setProfession("");
    setDescription("");
    setSystemPrompt("");
    setQuickPromptsText("");
  }, [open]);

  async function handleCreate() {
    if (!name.trim()) {
      notifications.notify({ tone: "error", title: "请先填写名称", message: "给这个专家起个名字" });
      return;
    }
    if (!systemPrompt.trim()) {
      notifications.notify({ tone: "error", title: "请先填写角色设定", message: "说说这个专家该怎么干活" });
      return;
    }
    const quickPrompts = quickPromptsText
      .split("\n")
      .map((s) => s.trim())
      .filter(Boolean);
    setSaving(true);
    try {
      const agent = await createExpert({
        name: name.trim(),
        description: description.trim(),
        systemPrompt: systemPrompt.trim(),
        // 工具/模型档位用默认（不向用户暴露）：tools 留空→运行期开放全部工具；模型用主模型。
        tools: [],
        modelTier: "main",
        displayName: displayName.trim() || null,
        profession: profession.trim() || null,
        avatar: null,
        quickPrompts,
      });
      onCreated(agent);
    } catch (err) {
      notifications.notify({ tone: "error", title: "创建失败", message: String(err) });
    } finally {
      setSaving(false);
    }
  }

  const field =
    "w-full rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-primary";

  return (
    <Modal open={open} onClose={onClose} title="新建专家">
      <ModalHeader onClose={onClose}>
        <h2 className="text-base font-semibold text-foreground">新建专家</h2>
        <p className="mt-0.5 text-xs text-foreground-muted">
          建好后，你可以在会话里选它来对话、让主助手把活交给它，或拉进团队当成员。
        </p>
      </ModalHeader>

      <div className="mt-4 space-y-3">
        <div className="flex gap-2">
          <input
            className={field}
            placeholder="名称（可用中文，如 小帮手）"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <input
            className={field}
            placeholder="显示名（可选，列表里展示的名字）"
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
          />
        </div>
        <input
          className={field}
          placeholder="职业/头衔（可选，如 首席分析师）"
          value={profession}
          onChange={(e) => setProfession(e.target.value)}
        />
        <input
          className={field}
          placeholder="一句话描述它能帮你干什么（方便选择时认出它）"
          value={description}
          onChange={(e) => setDescription(e.target.value)}
        />
        <textarea
          className={`${field} min-h-[140px] resize-y`}
          placeholder="角色设定：它是谁、该怎么干活、按什么格式给你结果…"
          value={systemPrompt}
          onChange={(e) => setSystemPrompt(e.target.value)}
        />
        <div>
          <textarea
            className={`${field} min-h-[72px] resize-y`}
            placeholder="用户引导语（可选，每行一条）：示范怎么用这个专家，如&#10;帮我分析这家公司的财报&#10;帮我把这段内容改写成小红书风格"
            value={quickPromptsText}
            onChange={(e) => setQuickPromptsText(e.target.value)}
          />
          <p className="mt-1 text-xs text-foreground-muted">
            每行一条；会显示在专家详情里，点一下就能带着它开始对话。
          </p>
        </div>
      </div>

      <div className="mt-4 flex items-center justify-end gap-2">
        <Button tone="outline" onClick={onClose} disabled={saving}>
          取消
        </Button>
        <Button tone="primary" onClick={() => void handleCreate()} disabled={saving}>
          {saving ? "创建中…" : "创建"}
        </Button>
      </div>
    </Modal>
  );
}
