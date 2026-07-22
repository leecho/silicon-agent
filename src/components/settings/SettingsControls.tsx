import type { InputHTMLAttributes, ReactNode } from "react";
import { Settings } from "lucide-react";
import { Button, Switch } from "../ui";

export { Switch };

export function SettingsSection({ title, description, children }: { title: string; description?: string; children: ReactNode }) {
  return (
    <section>
      <div className="mb-4">
        <h3 className="text-[15px] font-[650] leading-[1.4] text-foreground">{title}</h3>
        {description && (
          <p className="mt-1 text-[13px] leading-[1.6] text-foreground-secondary">
            {description}
          </p>
        )}
      </div>
      <div className="overflow-hidden rounded-lg border border-border bg-surface">{children}</div>
    </section>
  );
}

export function SettingItem({
  title,
  description,
  icon: Icon,
  children
}: {
  title: string;
  description: string;
  icon?: typeof Settings;
  children: ReactNode;
}) {
  return (
    <div className="grid min-h-[86px] grid-cols-[minmax(0,1fr)_minmax(220px,390px)] items-center gap-5 border-b border-border px-5 py-4 last:border-b-0">
      <div className="flex min-w-0 items-start gap-3">
        {Icon && (
          <span className="mt-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-md bg-accent text-foreground-muted">
            <Icon className="h-4 w-4" aria-hidden="true" />
          </span>
        )}
        <div className="min-w-0">
          <h4 className="truncate text-[13px] font-[650] leading-[1.45] text-foreground">
            {title}
          </h4>
          <p className="mt-1 line-clamp-2 text-[13px] leading-[1.6] text-foreground-secondary">
            {description}
          </p>
        </div>
      </div>
      <div className="flex min-w-0 justify-end">{children}</div>
    </div>
  );
}

export function ChoiceCard({
  icon: Icon,
  title,
  description,
  selected,
  onClick
}: {
  icon: typeof Settings;
  title: string;
  description: string;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      className={`grid min-h-24 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-4 rounded-lg border px-5 py-4 text-left transition ${
        selected ? "border-border bg-surface text-foreground" : "border-border bg-surface text-foreground-secondary hover:bg-accent hover:text-foreground"
      }`}
      type="button"
      onClick={onClick}
    >
      <span className="bg-accent p-2 rounded-md">
        <Icon className="h-5 w-5 text-foreground-secondary" aria-hidden="true" />
      </span>
      <span className="min-w-0">
        <span className="block truncate text-[13px] font-[650] leading-[1.45] text-foreground">
          {title}
        </span>
        <span className="mt-1 block text-[13px] leading-[1.6] text-foreground-secondary">
          {description}
        </span>
      </span>
      <span className={`grid h-5 w-5 place-items-center rounded-full border ${selected ? "border-primary bg-primary" : "border-border"}`}>
        {selected && <span className="h-2 w-2 rounded-full bg-white" />}
      </span>
    </button>
  );
}

export function TextInput({
  value,
  type = "text",
  placeholder,
  onChange,
  ...rest
}: {
  value: string;
  type?: string;
  placeholder?: string;
  onChange: (value: string) => void;
} & Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "type" | "placeholder" | "onChange">) {
  return (
    <input
      className="settings-input h-9 w-full rounded-lg border border-border bg-background px-3 font-[family-name:var(--app-font-family)] text-[13px] text-foreground outline-none transition placeholder:text-foreground-muted focus:border-ring"
      placeholder={placeholder}
      type={type}
      value={value}
      onChange={(event) => onChange(event.target.value)}
      {...rest}
    />
  );
}

export function NumberInput({ value, onChange }: { value: number; onChange: (value: number) => void }) {
  return (
    <input
      className="settings-input h-11 w-full rounded-lg border border-border bg-background px-3 font-[family-name:var(--app-font-family)] text-[13px] leading-[1.45] text-foreground outline-none transition focus:border-ring"
      max="2"
      min="0"
      step="0.1"
      type="number"
      value={value}
      onChange={(event) => onChange(Number(event.target.value))}
    />
  );
}

export function PrimaryButton({ disabled, icon: Icon, label, onClick }: { disabled: boolean; icon: typeof Settings; label: string; onClick: () => void }) {
  return (
    <Button className="w-fit justify-self-start px-4" tone="primary" onClick={onClick} disabled={disabled}>
      <Icon className="h-4 w-4" aria-hidden="true" />
      {label}
    </Button>
  );
}

export function SecondaryButton({ disabled, icon: Icon, label, onClick }: { disabled: boolean; icon: typeof Settings; label: string; onClick: () => void }) {
  return (
    <Button className="bg-card px-4" onClick={onClick} disabled={disabled}>
      <Icon className="h-4 w-4" aria-hidden="true" />
      {label}
    </Button>
  );
}

export function StatusBadge({ label, tone }: { label: string; tone: "success" | "warning" | "danger" }) {
  const color =
    tone === "success"
      ? "border border-success-border bg-success-subtle text-success"
      : tone === "warning"
        ? "border border-warning-border bg-warning-subtle text-warning"
        : "border border-danger-border bg-danger-subtle text-danger";
  return <span className={`rounded-md px-2.5 py-1.5 text-xs font-medium ${color}`}>{label}</span>;
}

export function ValuePill({ value }: { value: string }) {
  return (
    <span className="settings-input max-w-full truncate rounded-lg border border-border bg-background px-3 py-2 font-[family-name:var(--app-font-family)] text-[13px] leading-[1.45] text-foreground-secondary">
      {value}
    </span>
  );
}
