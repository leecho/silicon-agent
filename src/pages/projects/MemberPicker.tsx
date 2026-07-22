import { ExpertPickerDialog } from "../../components/experts/ExpertPickerDialog";
import type { ExpertSummary } from "../../types";

/** 成员选择弹框：搜索筛选 + 多选专家；以当前已选为初值，确定后回传完整名单（agent name）。 */
export function MemberPickerDialog({ agents, initial, onClose, onConfirm }: {
  agents: ExpertSummary[];
  initial: string[];
  onClose: () => void;
  onConfirm: (names: string[]) => void;
}) {
  return (
    <ExpertPickerDialog
      agents={agents}
      initial={initial}
      onClose={onClose}
      onConfirm={onConfirm}
      selectionMode="multiple"
      title="选择成员"
    />
  );
}
