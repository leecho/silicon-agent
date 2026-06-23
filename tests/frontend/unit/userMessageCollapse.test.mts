import { readFileSync } from "node:fs";

const source = readFileSync("src/components/session/MessageFeed.tsx", "utf8");

for (const required of [
  "USER_MESSAGE_COLLAPSE_CHAR_LIMIT",
  "USER_MESSAGE_COLLAPSE_LINE_LIMIT",
  "function isLongUserMessage",
  "function UserMessageBubble",
  "查看更多",
  "收起",
]) {
  if (!source.includes(required)) {
    throw new Error(`MessageFeed should support collapsible long user messages: missing ${required}`);
  }
}

if (!source.includes("max-h-32") || !source.includes("overflow-hidden")) {
  throw new Error("Collapsed user messages should cap visible height instead of occupying the full feed");
}

if (!source.includes("const [expanded, setExpanded] = useState(false)")) {
  throw new Error("Long user messages should be collapsed by default and expandable by the user");
}

