import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { FileText } from "lucide-react";
import type { Skill } from "../../types";
import { skillIcon } from "../../lib/skillPresentation";
import { Tooltip } from "../ui";

export interface ComposerInputHandle {
  /** 在光标处插入一个技能 chip（外部按钮如 [+] 调用）。 */
  insertSkill: (skill: Skill) => void;
  /** 用纯文本替换编辑区内容并聚焦（如点击快捷建议填入）。 */
  setText: (text: string) => void;
  focus: () => void;
  /** 清空编辑区。 */
  clear: () => void;
  /** 序列化编辑区文本（技能 chip → 技能：名）。附件由父组件单独管理。 */
  getText: () => string;
}

type MenuItem =
  | { type: "addFile" }
  | { type: "workspaceFile"; path: string }
  | { type: "skill"; skill: Skill };

type MentionTrigger = "/" | "@";

// 内联 chip（仅技能）：contenteditable=false 整块，可点 × 删除。
// data-chip-kind=skill，data-chip-value 为技能名，供序列化。
function makeChip(kind: "skill" | "file", value: string, label: string): HTMLSpanElement {
  const chip = document.createElement("span");
  chip.contentEditable = "false";
  chip.dataset.chipKind = kind;
  chip.dataset.chipValue = value;
  chip.className =
    "mx-0.5 inline-flex select-none items-center gap-1 rounded-md bg-muted px-1.5 py-1 align-middle text-xs text-foreground-secondary";
  const labelEl = document.createElement("span");
  labelEl.textContent = label;
  const remove = document.createElement("span");
  remove.dataset.chipRemove = "true";
  remove.textContent = "×";
  remove.className = "cursor-pointer px-0.5 text-foreground-muted hover:text-foreground";
  if (kind === "skill") {
    const iconEl = document.createElement("span");
    iconEl.ariaHidden = "true";
    iconEl.dataset.chipIcon = "skill";
    iconEl.textContent = "✦";
    iconEl.className = "grid h-3.5 w-3.5 shrink-0 place-items-center text-[10px] leading-none text-foreground-muted";
    chip.append(iconEl, labelEl, remove);
  } else {
    chip.append(labelEl, remove);
  }
  return chip;
}

// 递归序列化：文本节点取文本，chip 按类型还原，<br>/块元素换行。
function serializeNode(node: Node): string {
  if (node.nodeType === Node.TEXT_NODE) return node.textContent ?? "";
  if (node.nodeType !== Node.ELEMENT_NODE) return "";
  const el = node as HTMLElement;
  // 用 ⟦⟧（数学括号，用户几乎不会手打）包裹，供消息历史可靠地识别并还原为 chip 样式。
  if (el.dataset.chipKind === "skill") return `⟦技能：${el.dataset.chipValue ?? ""}⟧`;
  if (el.dataset.chipKind === "file") return `⟦@${el.dataset.chipValue ?? ""}⟧`;
  if (el.tagName === "BR") return "\n";
  let out = "";
  if (el.tagName === "DIV" && el.previousSibling) out += "\n";
  el.childNodes.forEach((c) => (out += serializeNode(c)));
  return out;
}

export const ComposerInput = forwardRef<ComposerInputHandle, {
  disabled?: boolean;
  skills: Skill[];
  maxHeightClassName?: string;
  minHeightClassName?: string;
  placeholder?: string;
  onSubmit: (text: string) => void;
  /** 内容变化回调：是否含可提交文本（供发送按钮启用/禁用）。 */
  onContentChange?: (hasText: boolean) => void;
  /** 会话工作区内的文件相对路径，供 @ 自动补全。 */
  workspaceFiles: string[];
  /** 在 / 菜单选「添加文件」：交由父组件弹选择器并加入顶部附件区。 */
  onRequestFile?: () => void;
  /** 粘贴/拖拽进来的文件或图片：交由父组件保存并加入顶部附件区。 */
  onPasteFiles?: (files: File[]) => void;
  /** 初始正文（含 ⟦技能：名⟧ 标记）；用于打开已存草稿时注水（仅挂载时填充一次）。 */
  initialContent?: string;
}>(function ComposerInput(
  {
    disabled,
    maxHeightClassName = "max-h-48",
    minHeightClassName = "min-h-[60px]",
    skills,
    workspaceFiles,
    placeholder,
    onSubmit,
    onContentChange,
    onRequestFile,
    onPasteFiles,
    initialContent,
  },
  ref,
) {
  const editorRef = useRef<HTMLDivElement | null>(null);
  // 最近一次落在编辑区内的选区（供外部按钮插入时恢复光标）。
  const savedRangeRef = useRef<Range | null>(null);
  // 当前 mention 触发的 token 上下文（选中菜单项时据此删除 /query 或 @query）。
  const mentionCtxRef = useRef<{
    trigger: MentionTrigger;
    textNode: Text;
    start: number;
    end: number;
  } | null>(null);

  const [isEmpty, setIsEmpty] = useState(true);
  const [menuOpen, setMenuOpen] = useState(false);
  const [trigger, setTrigger] = useState<MentionTrigger>("/");
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);

  const items = useMemo<MenuItem[]>(() => {
    const q = query.trim().toLowerCase();
    const matched = skills.filter(
      (s) =>
        !q ||
        s.name.toLowerCase().includes(q) ||
        (s.description ?? "").toLowerCase().includes(q),
    );
    const skillItems = matched.map((s) => ({ type: "skill" as const, skill: s }));
    if (trigger === "/") {
      return [{ type: "addFile" }, ...skillItems];
    }
    const matchedFiles = workspaceFiles
      .filter((path) => !q || path.toLowerCase().includes(q))
      .slice(0, 40)
      .map((path) => ({ type: "workspaceFile" as const, path }));
    return [...matchedFiles, ...skillItems];
  }, [query, skills, trigger, workspaceFiles]);

  useEffect(() => {
    setActiveIndex(0);
  }, [query]);

  // 记录编辑区内最近选区，供外部插入恢复光标。
  useEffect(() => {
    const onSel = () => {
      const sel = window.getSelection();
      if (sel && sel.rangeCount && editorRef.current?.contains(sel.anchorNode)) {
        savedRangeRef.current = sel.getRangeAt(0).cloneRange();
      }
    };
    document.addEventListener("selectionchange", onSel);
    return () => document.removeEventListener("selectionchange", onSel);
  }, []);

  const serialize = (): string => {
    const el = editorRef.current;
    if (!el) return "";
    let out = "";
    el.childNodes.forEach((n) => (out += serializeNode(n)));
    return out.replace(/ /g, " ").trim();
  };

  const updateEmpty = () => {
    const empty = serialize() === "";
    setIsEmpty(empty);
    onContentChange?.(!empty);
  };

  const closeMenu = () => {
    setMenuOpen(false);
    mentionCtxRef.current = null;
  };

  const clear = () => {
    if (editorRef.current) editorRef.current.innerHTML = "";
    closeMenu();
    setIsEmpty(true);
    onContentChange?.(false);
  };

  // 把含 ⟦技能：名⟧ 的正文还原为编辑区 DOM（文本节点 + 技能 chip）。
  const hydrate = (body: string) => {
    const el = editorRef.current;
    if (!el) return;
    el.innerHTML = "";
    const re = /⟦技能：([^⟧]+)⟧/g;
    let last = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(body)) !== null) {
      if (m.index > last) {
        el.appendChild(document.createTextNode(body.slice(last, m.index)));
      }
      el.appendChild(makeChip("skill", m[1], m[1]));
      last = m.index + m[0].length;
    }
    if (last < body.length) {
      el.appendChild(document.createTextNode(body.slice(last)));
    }
    updateEmpty();
  };

  // 仅挂载时注水一次（打开已存草稿）；initialContent 变化由父组件用 key 重挂控制。
  useEffect(() => {
    if (initialContent) hydrate(initialContent);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 聚焦并恢复上次选区；无有效选区则把光标移到末尾。
  const focusAndRestore = () => {
    const el = editorRef.current;
    if (!el) return;
    el.focus();
    const sel = window.getSelection();
    const saved = savedRangeRef.current;
    if (saved && el.contains(saved.startContainer)) {
      sel?.removeAllRanges();
      sel?.addRange(saved);
    } else {
      const r = document.createRange();
      r.selectNodeContents(el);
      r.collapse(false);
      sel?.removeAllRanges();
      sel?.addRange(r);
    }
  };

  // 在给定 range 处插入 chip + 尾随空格，并把光标移到空格后。
  const insertChipAt = (range: Range, chip: HTMLSpanElement) => {
    range.insertNode(chip);
    const space = document.createTextNode(" ");
    chip.after(space);
    const after = document.createRange();
    after.setStartAfter(space);
    after.collapse(true);
    const sel = window.getSelection();
    sel?.removeAllRanges();
    sel?.addRange(after);
    updateEmpty();
  };

  // 聚焦+恢复光标后，在当前选区插入一个 chip。
  const insertChipAtCaret = (chip: HTMLSpanElement) => {
    focusAndRestore();
    const sel = window.getSelection();
    if (!sel || !sel.rangeCount) return;
    const range = sel.getRangeAt(0);
    range.deleteContents();
    insertChipAt(range, chip);
  };

  useImperativeHandle(ref, () => ({
    insertSkill: (skill) => insertChipAtCaret(makeChip("skill", skill.name, skill.name)),
    setText: (text) => {
      const el = editorRef.current;
      if (!el) return;
      el.textContent = text;
      // 光标移到末尾。
      const sel = window.getSelection();
      const range = document.createRange();
      range.selectNodeContents(el);
      range.collapse(false);
      sel?.removeAllRanges();
      sel?.addRange(range);
      el.focus();
      updateEmpty();
    },
    focus: () => editorRef.current?.focus(),
    clear,
    getText: serialize,
  }));

  // 检测光标前是否存在 /token 或 @token（前面是行首或空白、且 token 内无空白）。
  const detectMention = () => {
    const sel = window.getSelection();
    if (!sel || sel.rangeCount === 0 || !sel.isCollapsed) return null;
    const range = sel.getRangeAt(0);
    const node = range.startContainer;
    if (node.nodeType !== Node.TEXT_NODE) return null;
    if (!editorRef.current?.contains(node)) return null;
    const text = node.textContent ?? "";
    const offset = range.startOffset;
    const before = text.slice(0, offset);
    const m = before.match(/(?:^|\s)([/@])([^\s@]*)$/);
    if (!m) return null;
    const mentionTrigger = m[1] as MentionTrigger;
    const q = m[2];
    return {
      trigger: mentionTrigger,
      textNode: node as Text,
      start: offset - q.length - 1,
      end: offset,
      query: q,
    };
  };

  const handleInput = () => {
    updateEmpty();
    const ctx = detectMention();
    if (ctx) {
      mentionCtxRef.current = {
        trigger: ctx.trigger,
        textNode: ctx.textNode,
        start: ctx.start,
        end: ctx.end,
      };
      setTrigger(ctx.trigger);
      setQuery(ctx.query);
      setMenuOpen(true);
    } else {
      closeMenu();
    }
  };

  // Enter 提交：把当前文本交给父组件，由父组件结合附件决定是否真正发送并清空
  //（含附件时即使正文为空也可发送）。
  const submit = () => {
    if (disabled) return;
    onSubmit(serialize());
  };

  // 选中菜单项：先删除 /query 或 @query，再插入对应 chip 或触发文件选择器。
  const selectItem = (item: MenuItem) => {
    const ctx = mentionCtxRef.current;
    let range: Range;
    const el = editorRef.current;
    if (ctx && el?.contains(ctx.textNode)) {
      range = document.createRange();
      range.setStart(ctx.textNode, ctx.start);
      range.setEnd(ctx.textNode, ctx.end);
      range.deleteContents();
      range.collapse(true);
    } else {
      focusAndRestore();
      const sel = window.getSelection();
      if (!sel || !sel.rangeCount) return;
      range = sel.getRangeAt(0);
    }
    closeMenu();
    if (item.type === "skill") {
      insertChipAt(range, makeChip("skill", item.skill.name, item.skill.name));
    } else if (item.type === "workspaceFile") {
      insertChipAt(range, makeChip("file", item.path, item.path));
    } else {
      // 文件：/query 已删除，交给父组件弹选择器并加入顶部附件区（不插入编辑区）。
      range.collapse(true);
      onRequestFile?.();
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (menuOpen && items.length > 0) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setActiveIndex((i) => (i + 1) % items.length);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setActiveIndex((i) => (i - 1 + items.length) % items.length);
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        selectItem(items[activeIndex]);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        closeMenu();
        return;
      }
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
      return;
    }
    if (e.key === "Enter" && e.shiftKey) {
      e.preventDefault();
      document.execCommand("insertLineBreak");
      updateEmpty();
    }
  };

  // 点击 chip 的 × 删除整块。
  const handleClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    if (target.dataset.chipRemove !== undefined) {
      e.preventDefault();
      target.closest("[data-chip-kind]")?.remove();
      updateEmpty();
      editorRef.current?.focus();
    }
  };

  // 从剪贴板收集文件（含粘贴的图片：部分浏览器走 items.getAsFile）。
  const clipboardFiles = (dt: DataTransfer): File[] => {
    if (dt.files && dt.files.length > 0) return Array.from(dt.files);
    const out: File[] = [];
    for (const it of Array.from(dt.items ?? [])) {
      if (it.kind === "file") {
        const f = it.getAsFile();
        if (f) out.push(f);
      }
    }
    return out;
  };

  // 粘贴：
  // 1) 含文件/图片 → 写入工作目录并插入文件 chip（onPasteFile）。
  // 2) 否则 contenteditable 默认会插入富文本 HTML，破坏「纯文本 + chip」模型；
  //    拦截后只取 text/plain，并剥掉 chip 哨兵 ⟦⟧，防止粘贴内容伪造 chip。
  const handlePaste = (e: React.ClipboardEvent<HTMLDivElement>) => {
    const files = onPasteFiles ? clipboardFiles(e.clipboardData) : [];
    if (files.length > 0) {
      e.preventDefault();
      onPasteFiles!(files);
      return;
    }
    e.preventDefault();
    const text = e.clipboardData.getData("text/plain");
    if (!text) return;
    const clean = text.replace(/[⟦⟧]/g, "");
    document.execCommand("insertText", false, clean);
    updateEmpty();
  };

  return (
    <div className="relative">
      {menuOpen && (
        <div className="max-w-lg absolute bottom-full left-0 right-0 z-30 mb-2 max-h-72 overflow-auto rounded-xl border border-border bg-popover p-1 text-popover-foreground shadow-lg">
          {trigger === "/" ? (
            <>
              <MenuSection label="附件" />
              {items.map((item, i) =>
                item.type === "addFile" ? (
                  <MenuRow
                    key="add-file"
                    active={activeIndex === i}
                    icon={<FileText className="h-3.5 w-3.5 shrink-0 text-foreground-secondary" />}
                    title="添加文件"
                    description="选择本地文件，插入 @路径"
                    onSelect={() => selectItem(item)}
                  />
                ) : null,
              )}
            </>
          ) : (
            <>
              <MenuSection label="工作区文件" />
              {items.filter((it) => it.type === "workspaceFile").length === 0 ? (
                <div className="px-2.5 py-1.5 text-[12px] text-foreground-muted">
                  暂无匹配文件
                </div>
              ) : (
                items.map((item, i) =>
                  item.type === "workspaceFile" ? (
                    <MenuRow
                      key={item.path}
                      active={activeIndex === i}
                      icon={<FileText className="h-3.5 w-3.5 shrink-0 text-foreground-secondary" />}
                      title={item.path}
                      description="工作区文件"
                      onSelect={() => selectItem(item)}
                    />
                  ) : null,
                )
              )}
            </>
          )}
          <MenuSection label="技能" />
          {items.filter((it) => it.type === "skill").length === 0 ? (
            <div className="px-2.5 py-1.5 text-[12px] text-foreground-muted">
              暂无匹配技能
            </div>
          ) : (
            items.map((item, i) => {
              if (item.type !== "skill") return null;
              const Icon = skillIcon(item.skill);
              return (
                <MenuRow
                  key={item.skill.id}
                  active={activeIndex === i}
                  icon={<Icon className="h-3.5 w-3.5 shrink-0 text-foreground-secondary" />}
                  title={item.skill.name}
                  description={item.skill.description || undefined}
                  onSelect={() => selectItem(item)}
                />
              );
            })
          )}
        </div>
      )}
      {isEmpty && (
        <div className="pointer-events-none absolute left-3 top-3 text-sm text-foreground-muted">
          {placeholder ?? "描述任务，/ 添加附件或技能，@ 引用文件或技能"}
        </div>
      )}
      <div
        ref={editorRef}
        role="textbox"
        aria-multiline="true"
        contentEditable={!disabled}
        suppressContentEditableWarning
        className={`block ${maxHeightClassName} ${minHeightClassName} w-full overflow-auto whitespace-pre-wrap px-3 pt-3 text-sm text-foreground outline-none [overflow-wrap:anywhere]`}
        onInput={handleInput}
        onKeyDown={handleKeyDown}
        onClick={handleClick}
        onPaste={handlePaste}
      />
    </div>
  );
});

function MenuSection({ label }: { label: string }) {
  return (
    <div className="px-2.5 pb-1 pt-1.5 text-[11px] font-medium text-foreground-muted">
      {label}
    </div>
  );
}

function MenuRow({
  active,
  icon,
  title,
  description,
  onSelect,
}: {
  active: boolean;
  icon: React.ReactNode;
  title: string;
  description?: string;
  onSelect: () => void;
}) {
  return (
    <Tooltip content={description}>
    <button
      type="button"
      // onMouseDown + preventDefault：避免点击时编辑区先失焦，保住选区。
      onMouseDown={(e) => {
        e.preventDefault();
        onSelect();
      }}
      className={
        "flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left transition " +
        (active ? "bg-accent" : "hover:bg-accent")
      }
    >
      <span>{icon}</span>
      <span className="truncate text-[13px] text-foreground">{title}</span>
      {description && (
          <span className="flex-1 block truncate text-[11px] text-foreground-muted">
            {description}
          </span>
        )}
      </button>
      </Tooltip>
  );
}
