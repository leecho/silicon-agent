import type { ComponentType, ReactNode } from "react";
import { isValidElement } from "react";
import { joinClasses } from "./utils";

/**
 * 空态 / 错误态通用卡片（四态中的「空」与「错误」）。
 * icon 同时支持组件类型（icon={Foo}）与 JSX 元素（icon={<Foo/>}），兼容历史调用。
 */
export function EmptyState({
  icon,
  title,
  description,
  action,
  variant = "empty",
  className,
}: {
  icon?: ReactNode | ComponentType<{ className?: string }>;
  title: string;
  description?: ReactNode;
  action?: ReactNode;
  variant?: "empty" | "error";
  className?: string;
}) {
  const tone = variant === "error" ? "text-danger" : "text-foreground-muted";
  let iconNode: ReactNode = null;
  // 组件类型（函数组件，或 lucide 等 forwardRef/memo 对象）→ 实例化；
  // 已创建的 JSX 元素 / emoji 字符串 → 作为子节点渲染。
  const isComponentType =
    typeof icon === "function" || (typeof icon === "object" && icon !== null && !isValidElement(icon));
  if (isComponentType) {
    const Icon = icon as ComponentType<{ className?: string }>;
    iconNode = <Icon className={joinClasses("h-10 w-10", tone)} />;
  } else if (icon) {
    iconNode = (
      <div className={joinClasses("flex h-10 w-10 items-center justify-center text-2xl", tone)}>{icon}</div>
    );
  }

  return (
    <div
      className={joinClasses(
        "flex flex-col items-center justify-center gap-3 px-4 py-12 text-center",
        className,
      )}
    >
      {iconNode}
      <h3
        className={joinClasses(
          "text-base font-semibold",
          variant === "error" ? "text-danger" : "text-foreground",
        )}
      >
        {title}
      </h3>
      {description ? <p className="max-w-sm text-sm text-foreground-secondary">{description}</p> : null}
      {action ? <div className="mt-2">{action}</div> : null}
    </div>
  );
}
