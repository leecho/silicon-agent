import type { ReactNode } from "react";
import { joinClasses } from "./utils";

export function Tabs<T extends string>({
  items,
  onChange,
  value
}: {
  items: Array<{ label: ReactNode; value: T }>;
  onChange: (value: T) => void;
  value: T;
}) {
  return (
    <nav className="flex min-w-0 gap-1" role="tablist">
      {items.map((item) => {
        const active = item.value === value;
        return (
          <button
            aria-selected={active}
            className={joinClasses(
              "rounded-lg px-3 py-2 text-sm transition",
              active ? "bg-accent text-accent-foreground" : "text-foreground-secondary hover:bg-accent hover:text-accent-foreground"
            )}
            key={item.value}
            role="tab"
            type="button"
            onClick={() => onChange(item.value)}
          >
            {item.label}
          </button>
        );
      })}
    </nav>
  );
}
