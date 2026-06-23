import { useState, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";

// 统一折叠组件：state 驱动展开（forceOpen 可强制展开，如运行中/思考中）；
// chevron 按 isOpen 旋转；可选 leading icon + label + 等宽内容；内容区带左边框缩进。
export function Disclosure({
  icon,
  label,
  labelClassName,
  defaultOpen,
  forceOpen,
  mono,
  children,
}: {
  icon?: ReactNode;
  label: ReactNode;
  labelClassName?: string;
  defaultOpen?: boolean;
  forceOpen?: boolean;
  mono?: boolean;
  children?: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen ?? false);
  const isOpen = forceOpen ? true : open;
  return (
    <div className="min-w-0 max-w-full py-1">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className={`flex min-w-0 max-w-full items-center gap-1.5 text-[13px] font-medium text-foreground-muted transition hover:text-foreground-secondary ${labelClassName ?? ""}`}
      >
        {icon}
        <span className="truncate">{label}</span>
        <ChevronRight
          className={`h-3.5 w-3.5 shrink-0 transition ${isOpen ? "rotate-90" : ""}`}
          aria-hidden="true"
        />
      </button>
      {isOpen && (
        <div
          className={`mt-1 min-w-0 max-w-full whitespace-pre-wrap break-words border-l border-border-subtle px-3 py-1 text-[12px] leading-5 text-foreground-muted${mono ? " font-mono" : ""}`}
        >
          {children}
        </div>
      )}
    </div>
  );
}
