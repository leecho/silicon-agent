import type { InputHTMLAttributes, ReactNode } from "react";

/** 设置页通用文本字段：标题 + 可选说明 + 受控输入。 */
export function TextField({
  label,
  description,
  value,
  onChange,
  type = "text",
  placeholder,
  autoComplete,
  ...rest
}: {
  label: string;
  description?: ReactNode;
  value: string;
  onChange: (value: string) => void;
} & Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange">) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-sm font-medium text-foreground">{label}</span>
      {description && <span className="text-xs text-foreground-secondary">{description}</span>}
      <input
        autoComplete={autoComplete}
        className="h-10 w-full rounded-lg border border-input bg-background px-3 text-sm text-foreground placeholder:text-foreground-muted outline-none transition focus:border-ring"
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        type={type}
        value={value}
        {...rest}
      />
    </label>
  );
}
