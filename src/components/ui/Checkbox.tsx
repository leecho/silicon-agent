import { Check } from "lucide-react";
import type { ButtonHTMLAttributes } from "react";
import { joinClasses } from "./utils";

export function Checkbox({
  checked,
  className,
  disabled = false,
  onChange,
  ...props
}: Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onChange" | "role"> & {
  checked: boolean;
  disabled?: boolean;
  onChange?: (checked: boolean) => void;
}) {
  return (
    <button
      aria-checked={checked}
      className={joinClasses(
        "grid h-5 w-5 shrink-0 place-items-center rounded-md border text-primary-foreground transition focus:outline-none focus:ring-2 focus:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-60",
        checked
          ? "border-primary bg-primary hover:brightness-110"
          : "border-input bg-background text-transparent hover:border-ring hover:bg-accent",
        className,
      )}
      disabled={disabled}
      role="checkbox"
      type="button"
      onClick={() => onChange?.(!checked)}
      {...props}
    >
      <Check className="h-3.5 w-3.5" aria-hidden="true" strokeWidth={3} />
    </button>
  );
}
