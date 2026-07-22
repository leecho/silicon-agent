import { joinClasses } from "./utils";

/**
 * 骨架屏占位（四态中的「加载中」），保持布局稳定。
 * 传 lines>1 时渲染多行等高骨架条；否则单块（用 className 控制尺寸）。
 */
export function Skeleton({ className, lines }: { className?: string; lines?: number }) {
  if (lines !== undefined && lines > 1) {
    return (
      <div className={joinClasses("flex flex-col gap-2", className)}>
        {Array.from({ length: lines }).map((_, i) => (
          <div
            key={i}
            className={joinClasses(
              "h-4 animate-pulse rounded-md bg-muted",
              i === lines - 1 ? "w-3/4" : "w-full",
            )}
          />
        ))}
      </div>
    );
  }

  return <div className={joinClasses("h-4 w-full animate-pulse rounded-md bg-muted", className)} />;
}

/** 列表骨架：渲染 n 行等高骨架条。 */
export function SkeletonList({ rows = 4, className }: { rows?: number; className?: string }) {
  return (
    <div className={joinClasses("flex flex-col gap-3", className)}>
      {Array.from({ length: rows }).map((_, i) => (
        <Skeleton key={i} className="h-16 w-full" />
      ))}
    </div>
  );
}
