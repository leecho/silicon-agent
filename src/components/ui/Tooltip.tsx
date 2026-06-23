import {
  cloneElement,
  isValidElement,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
  type FocusEvent,
  type HTMLAttributes,
  type MouseEvent,
  type ReactElement,
  type ReactNode,
  type Ref
} from "react";
import { createPortal } from "react-dom";
import { joinClasses } from "./utils";

type TooltipChild = ReactElement<HTMLAttributes<HTMLElement> & { ref?: Ref<HTMLElement> }>;

type TooltipRect = {
  left: number;
  placement: "bottom" | "top";
  top: number;
};

const TOOLTIP_GAP = 8;
const TOOLTIP_VIEWPORT_GAP = 12;
const TOOLTIP_ESTIMATED_WIDTH = 240;
const TOOLTIP_ESTIMATED_HEIGHT = 32;

function clampTooltipPosition({
  placement,
  preferredLeft,
  triggerBottom,
  triggerTop,
  tooltipHeight,
  tooltipWidth
}: {
  placement: TooltipRect["placement"];
  preferredLeft: number;
  triggerBottom: number;
  triggerTop: number;
  tooltipHeight: number;
  tooltipWidth: number;
}): TooltipRect {
  const minLeft = TOOLTIP_VIEWPORT_GAP;
  const maxLeft = Math.max(
    minLeft,
    window.innerWidth - TOOLTIP_VIEWPORT_GAP - tooltipWidth
  );
  const left = Math.min(
    Math.max(preferredLeft - tooltipWidth / 2, minLeft),
    maxLeft
  );
  const rawTop =
    placement === "top"
      ? triggerTop - TOOLTIP_GAP - tooltipHeight
      : triggerBottom + TOOLTIP_GAP;
  const minTop = TOOLTIP_VIEWPORT_GAP;
  const maxTop = Math.max(
    minTop,
    window.innerHeight - TOOLTIP_VIEWPORT_GAP - tooltipHeight
  );
  const top = Math.min(Math.max(rawTop, minTop), maxTop);

  return { left, placement, top };
}

export function Tooltip({
  children,
  className,
  content,
  disabled = false
}: {
  children: TooltipChild;
  className?: string;
  content: ReactNode;
  disabled?: boolean;
}) {
  const id = useId();
  const triggerRef = useRef<HTMLElement | null>(null);
  const tooltipRef = useRef<HTMLDivElement | null>(null);
  const [open, setOpen] = useState(false);
  const [rect, setRect] = useState<TooltipRect | null>(null);

  const updateRect = useCallback(() => {
    const trigger = triggerRef.current;
    if (!trigger) return;

    const triggerRect = trigger.getBoundingClientRect();
    const placement = triggerRect.top >= 40 ? "top" : "bottom";
    const preferredLeft = triggerRect.left + triggerRect.width / 2;
    const tooltip = tooltipRef.current;
    const tooltipWidth = tooltip?.offsetWidth ?? TOOLTIP_ESTIMATED_WIDTH;
    const tooltipHeight = tooltip?.offsetHeight ?? TOOLTIP_ESTIMATED_HEIGHT;

    setRect(
      clampTooltipPosition({
        placement,
        preferredLeft,
        tooltipHeight,
        tooltipWidth,
        triggerBottom: triggerRect.bottom,
        triggerTop: triggerRect.top
      })
    );
  }, []);

  useEffect(() => {
    if (!open) return;
    updateRect();

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }

    function handlePositionChange() {
      updateRect();
    }

    document.addEventListener("keydown", handleKeyDown);
    window.addEventListener("resize", handlePositionChange);
    window.addEventListener("scroll", handlePositionChange, true);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("resize", handlePositionChange);
      window.removeEventListener("scroll", handlePositionChange, true);
    };
  }, [open, updateRect]);

  useEffect(() => {
    if (open) updateRect();
  }, [open, content, updateRect]);

  if (!isValidElement(children)) return null;

  const child = children as TooltipChild;
  const childRef = (child as unknown as { ref?: Ref<HTMLElement> }).ref;
  const childProps = child.props;

  function setTriggerRef(node: HTMLElement | null) {
    triggerRef.current = node;
    if (typeof childRef === "function") childRef(node);
    else if (childRef && "current" in childRef) {
      (childRef as { current: HTMLElement | null }).current = node;
    }
  }

  function show() {
    if (disabled || !content) return;
    updateRect();
    setOpen(true);
  }

  function hide() {
    setOpen(false);
  }

  const trigger = cloneElement(child, {
    "aria-describedby": open ? id : childProps["aria-describedby"],
    onBlur: (event: FocusEvent<HTMLElement>) => {
      childProps.onBlur?.(event);
      hide();
    },
    onFocus: (event: FocusEvent<HTMLElement>) => {
      childProps.onFocus?.(event);
      show();
    },
    onMouseEnter: (event: MouseEvent<HTMLElement>) => {
      childProps.onMouseEnter?.(event);
      show();
    },
    onMouseLeave: (event: MouseEvent<HTMLElement>) => {
      childProps.onMouseLeave?.(event);
      hide();
    },
    ref: setTriggerRef,
    title: undefined
  });
  const portalTarget = triggerRef.current?.closest(".theme-light, .theme-dark") ?? document.body;

  return (
    <>
      {trigger}
      {open && rect && createPortal(
        <div
          ref={tooltipRef}
          id={id}
          role="tooltip"
          className={joinClasses(
            "pointer-events-none break-words fixed z-[120] max-w-[360px] rounded-md border border-border bg-popover px-2 py-1 text-xs font-medium text-popover-foreground shadow-xl",
            className
          )}
          style={{
            left: rect.left,
            top: rect.top
          }}
        >
          {content}
        </div>,
        portalTarget
      )}
    </>
  );
}
