import { joinClasses } from "./utils";

export function Switch({
  checked,
  disabled = false,
  onChange
}: {
  checked: boolean;
  disabled?: boolean;
  onChange?: (checked: boolean) => void;
}) {
  return (
    <button
      aria-checked={checked}
      className={joinClasses(
        "relative h-6 w-11 rounded-full transition",
        checked ? "bg-primary" : "bg-accent",
        disabled ? "opacity-70" : "hover:brightness-110"
      )}
      disabled={disabled}
      role="switch"
      type="button"
      onClick={() => onChange?.(!checked)}
    >
      <span className={joinClasses("absolute top-1 h-4 w-4 rounded-full bg-white shadow transition", checked ? "left-6" : "left-1")} />
    </button>
  );
}
