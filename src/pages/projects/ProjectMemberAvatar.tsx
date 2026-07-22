import { Bot } from "lucide-react";

import { avatarEmoji } from "../../lib/avatar";
import type { ProjectMember } from "../../types";

export function ProjectMemberAvatar({ member }: { member: ProjectMember }) {
  return (
    <span className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-border bg-background text-[15px]">
      {avatarEmoji(member.avatar) ? (
        <span aria-hidden="true">{avatarEmoji(member.avatar)}</span>
      ) : (
        <Bot className="h-3.5 w-3.5 text-foreground-muted" aria-hidden="true" />
      )}
    </span>
  );
}
