import { ChevronDown, Zap } from "lucide-react";
import { DropdownMenu, DropdownMenuItem, Tooltip } from "../../../components/ui";
import type { EnabledProviderModels } from "../../../types";
import { useAnchoredMenu } from "./useAnchoredMenu";

// 模型选择下拉：selectedModelId=null 时直接选中全局默认模型。
export function ModelPicker({
  modelGroups,
  selectedModelId,
  onPick,
}: {
  modelGroups: EnabledProviderModels[];
  selectedModelId: string | null;
  onPick: (modelId: string | null) => void;
}) {
  const { anchorRect, open, triggerRef, toggle, close } = useAnchoredMenu();
  const all = modelGroups.flatMap((g) => g.models);
  const defaultModel = all.find((m) => m.isDefault) ?? null;
  const selected =
    selectedModelId == null ? defaultModel : all.find((m) => m.id === selectedModelId);
  const label =
    selected
      ? selected.displayName || selected.model
      : selectedModelId == null
        ? "默认模型"
        : "已停用";

  function choose(modelId: string) {
    close();
    onPick(modelId === defaultModel?.id ? null : modelId);
  }

  return (
    <>
      <Tooltip content="选择模型">
        <button
          ref={triggerRef}
          type="button"
          className="flex items-center gap-1 rounded-md px-2 py-1.5 text-xs text-foreground-secondary hover:bg-accent"
          onClick={(e) => {
            e.stopPropagation();
            toggle();
          }}
        >
          <Zap className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          <span className="max-w-[140px] truncate">{label}</span>
          <ChevronDown className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
        </button>
      </Tooltip>
      {open && (
        <DropdownMenu
          align="end"
          anchorElement={triggerRef.current}
          anchorRect={anchorRect}
          onClose={close}
          placement="top"
          width={220}
          items={[
            {
              id: "model-list",
              type: "custom",
              render: (
                <div className="max-h-[300px] overflow-auto">
                  {modelGroups.map((g) => (
                    <div key={g.providerId}>
                      <div className="px-2.5 pb-1 pt-2 text-[11px] font-medium text-foreground-muted">
                        {g.providerName}
                      </div>
                      {g.models.map((m) => (
                        <DropdownMenuItem
                          key={m.id}
                          icon={Zap}
                          label={m.displayName || m.model}
                          selected={m.id === selected?.id}
                          onClick={() => choose(m.id)}
                        />
                      ))}
                    </div>
                  ))}
                </div>
              ),
            },
          ]}
        />
      )}
    </>
  );
}
