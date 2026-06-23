import { PanelRightClose } from "lucide-react";
import type { Artifact, TodoItem } from "../../types";
import { WorkspacePanel } from "./SessionArtifact";
import { TodoPanel } from "./SessionTodo";
import { Tooltip } from "../ui/Tooltip";

export function SessionMonitorPanel({
  artifacts,
  onCollapse,
  onOpenArtifact,
  onOpenWorkspace,
  todos,
  workspaceLabel,
  workspacePath,
}: {
  artifacts: Artifact[];
  onCollapse: () => void;
  onOpenArtifact: (artifact: Artifact) => void;
  onOpenWorkspace: () => void;
  todos: TodoItem[];
  workspaceLabel: string;
  workspacePath?: string;
}) {
  return (
    <div className="flex h-full w-[300px] flex-col gap-5 overflow-auto border-l border-border-subtle bg-surface px-3 pb-3 text-card-foreground">
      <div className="flex items-center justify-between gap-2 pt-1">
        <div className="text-sm font-semibold text-foreground">任务监控</div>
        <Tooltip content="收起任务监控">
          <button
            type="button"
            aria-label="收起任务监控"
            className="grid h-8 w-8 shrink-0 place-items-center rounded-md text-foreground-secondary transition hover:bg-accent hover:text-foreground"
            onClick={onCollapse}
          >
            <PanelRightClose className="h-[14px] w-[14px]" aria-hidden="true" />
          </button>
        </Tooltip>
      </div>
      <div className="flex-1 overflow-auto flex flex-col gap-3">
      <TodoPanel todos={todos} />
      <WorkspacePanel
        label={workspaceLabel}
        fullPath={workspacePath}
        onOpen={onOpenWorkspace}
        artifacts={artifacts}
        onOpenArtifact={onOpenArtifact}
      />
      </div>
      </div>
  );
}
