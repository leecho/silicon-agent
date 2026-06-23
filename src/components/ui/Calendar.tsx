import { CalendarDays, ChevronLeft, ChevronRight } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { joinClasses } from "./utils";

const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"];

export function Calendar({
  label,
  onChange,
  value
}: {
  label: string;
  onChange: (value: string) => void;
  value: string;
}) {
  const [open, setOpen] = useState(false);
  const [visibleMonth, setVisibleMonth] = useState(() => monthStart(parseDate(value) ?? new Date()));
  const rootRef = useRef<HTMLDivElement>(null);
  const selectedDate = parseDate(value);
  const days = useMemo(() => daysForMonth(visibleMonth), [visibleMonth.getFullYear(), visibleMonth.getMonth()]);

  useEffect(() => {
    if (!open) return;
    setVisibleMonth(monthStart(selectedDate ?? new Date()));
  }, [open, selectedDate?.getTime()]);

  useEffect(() => {
    if (!open) return;

    function handlePointerDown(event: PointerEvent) {
      if (rootRef.current?.contains(event.target as Node)) return;
      setOpen(false);
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }

    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  function selectDay(date: Date) {
    onChange(formatDate(date));
    setOpen(false);
  }

  return (
    <div className="relative grid gap-1 text-xs font-medium text-foreground-secondary" ref={rootRef}>
      {label}
      <button
        className="flex h-10 min-w-[148px] items-center justify-between gap-2 rounded-lg border border-input bg-background px-3 text-left text-sm text-foreground outline-none transition hover:bg-accent focus:border-ring"
        type="button"
        onClick={() => setOpen((current) => !current)}
      >
        <span>{value || "选择日期"}</span>
        <CalendarDays className="h-4 w-4 text-foreground-muted" aria-hidden="true" />
      </button>
      {open && (
        <div className="absolute right-0 top-[calc(100%+6px)] z-50 w-72 rounded-lg border border-border bg-popover p-3 text-popover-foreground shadow-2xl">
          <div className="mb-3 flex items-center justify-between">
            <button className="grid h-8 w-8 place-items-center rounded-lg text-foreground-secondary transition hover:bg-accent hover:text-accent-foreground" type="button" onClick={() => setVisibleMonth(addMonths(visibleMonth, -1))}>
              <ChevronLeft className="h-4 w-4" aria-hidden="true" />
            </button>
            <strong className="text-sm font-semibold text-popover-foreground">{visibleMonth.getFullYear()} 年 {visibleMonth.getMonth() + 1} 月</strong>
            <button className="grid h-8 w-8 place-items-center rounded-lg text-foreground-secondary transition hover:bg-accent hover:text-accent-foreground" type="button" onClick={() => setVisibleMonth(addMonths(visibleMonth, 1))}>
              <ChevronRight className="h-4 w-4" aria-hidden="true" />
            </button>
          </div>
          <div className="grid grid-cols-7 gap-1 text-center text-[11px] text-foreground-muted">
            {WEEKDAYS.map((day) => (
              <span className="py-1" key={day}>{day}</span>
            ))}
          </div>
          <div className="mt-1 grid grid-cols-7 gap-1">
            {days.map((date, index) => {
              if (!date) return <span className="h-8" key={`empty-${index}`} />;
              const active = selectedDate ? sameDay(selectedDate, date) : false;
              const today = sameDay(new Date(), date);
              return (
                <button
                  className={joinClasses(
                    "grid h-8 place-items-center rounded-lg text-sm transition",
                    active
                      ? "bg-primary text-primary-foreground"
                      : today
                        ? "bg-accent text-accent-foreground"
                        : "text-foreground-secondary hover:bg-accent hover:text-accent-foreground"
                  )}
                  key={formatDate(date)}
                  type="button"
                  onClick={() => selectDay(date)}
                >
                  {date.getDate()}
                </button>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

function parseDate(value: string) {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(year, month - 1, day);
  if (date.getFullYear() !== year || date.getMonth() !== month - 1 || date.getDate() !== day) return null;
  return date;
}

function formatDate(date: Date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function monthStart(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), 1);
}

function addMonths(date: Date, offset: number) {
  return new Date(date.getFullYear(), date.getMonth() + offset, 1);
}

function daysForMonth(month: Date) {
  const first = monthStart(month);
  const startOffset = (first.getDay() + 6) % 7;
  const dayCount = new Date(first.getFullYear(), first.getMonth() + 1, 0).getDate();
  const result: Array<Date | null> = Array.from({ length: startOffset }, () => null);
  for (let day = 1; day <= dayCount; day += 1) {
    result.push(new Date(first.getFullYear(), first.getMonth(), day));
  }
  return result;
}

function sameDay(left: Date, right: Date) {
  return left.getFullYear() === right.getFullYear() && left.getMonth() === right.getMonth() && left.getDate() === right.getDate();
}
