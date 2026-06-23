import { useRef, useState } from "react";
import { ChevronDown, SmilePlus } from "lucide-react";
import { DropdownMenu, type DropdownMenuAnchor } from "./DropdownMenu";
import { joinClasses } from "./utils";

export const DEFAULT_AVATAR_EMOJIS = [
  "🤖", "🧠", "🧑‍💻", "👩‍💻", "👨‍💻", "🧑‍🔬", "🧑‍🎨", "🧑‍🏫", "🧑‍💼", "🕵️",
  "🧙‍♂️", "🧭", "📈", "✍️", "⚖️", "🛠️", "🎯", "🚀", "💡", "🔍",
  "📚", "🗂️", "🧪", "🎨", "🏗️", "🛡️", "💬", "📊", "🧩", "⭐",
];

export function EmojiPicker({
  className,
  label = "头像 emoji",
  onChange,
  options = DEFAULT_AVATAR_EMOJIS,
  value,
}: {
  className?: string;
  label?: string;
  onChange: (value: string) => void;
  options?: string[];
  value: string;
}) {
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const [open, setOpen] = useState(false);
  const [anchorRect, setAnchorRect] = useState<DropdownMenuAnchor | null>(null);
  const preview = value.trim();

  function toggleMenu() {
    const rect = triggerRef.current?.getBoundingClientRect();
    if (!rect) return;
    setAnchorRect({ bottom: rect.bottom, left: rect.left, right: rect.right, top: rect.top });
    setOpen((current) => !current);
  }

  function pick(item: string) {
    onChange(item);
    setOpen(false);
  }

  return (
    <div className={joinClasses("space-y-2", className)}>
      <div>
      {label && <label className="text-[13px] font-medium text-foreground-secondary">{label}</label>}
      </div>
        <button
        ref={triggerRef}
        type="button"
        className="inline-flex h-10 min-w-20 items-center justify-between gap-2 rounded-xl border border-border bg-background px-3 text-left text-sm text-foreground transition hover:bg-accent hover:text-foreground"
        onClick={toggleMenu}
      >
        <span className="grid h-7 w-7 shrink-0 place-items-center text-xl">
          {preview || <SmilePlus className="h-5 w-5 text-foreground-muted" aria-hidden="true" />}
        </span>
        <ChevronDown className="h-3.5 w-3.5 shrink-0 text-foreground-muted" aria-hidden="true" />
      </button>
      {open && (
        <DropdownMenu
          align="end"
          anchorElement={triggerRef.current}
          anchorRect={anchorRect}
          onClose={() => setOpen(false)}
          placement="bottom"
          width={268}
        >
          <div className="grid grid-cols-8 gap-1 p-1">
            {options.map((item) => (
              <button
                key={item}
                type="button"
                onClick={() => pick(item)}
                className={joinClasses(
                  "grid h-8 w-8 place-items-center rounded-lg border text-[17px] transition",
                  preview === item
                    ? "border-primary bg-primary/10"
                    : "border-border-subtle bg-background hover:border-border",
                )}
                title={`选择 ${item}`}
              >
                {item}
              </button>
            ))}
          </div>
        </DropdownMenu>
      )}
    </div>
  );
}
