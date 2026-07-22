import type { ReactNode } from "react";

export function EmptyState({ icon, title, hint }: { icon: ReactNode; title: string; hint?: string }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 rounded-xl border border-dashed border-border py-16 text-foreground-muted">
      <div className="grid h-12 w-12 place-items-center rounded-full bg-muted">{icon}</div>
      <p className="text-sm text-foreground-secondary">{title}</p>
      {hint && <p className="max-w-[340px] text-center text-xs">{hint}</p>}
    </div>
  );
}
