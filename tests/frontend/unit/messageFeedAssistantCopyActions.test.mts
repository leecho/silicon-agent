import { readFileSync } from "node:fs";

const messageFeedSource = readFileSync("src/components/session/MessageFeed.tsx", "utf8");
const source = readFileSync("src/components/session/AssistantAnswer.tsx", "utf8");

if (!source.includes("function AssistantAnswer(")) {
  throw new Error("AssistantAnswer component should render assistant final answers");
}

if (!messageFeedSource.includes("<AssistantAnswer markdown={row.content} />")) {
  throw new Error("MessageFeed should render assistant final answers through AssistantAnswer");
}

if (!source.includes("answerRef")) {
  throw new Error("AssistantAnswer should keep a ref to the rendered answer for plain-text copy");
}

if (!source.includes("navigator.clipboard.writeText(text)")) {
  throw new Error("AssistantAnswer should copy rendered plain text to the clipboard");
}

if (!source.includes("navigator.clipboard.writeText(markdown)")) {
  throw new Error("AssistantAnswer should copy the raw markdown to the clipboard");
}

if (!source.includes('复制成Markdown')) {
  throw new Error("AssistantAnswer should expose a Copy as Markdown action");
}

if (!source.includes('title: "已复制"') || !source.includes('title: "复制失败"')) {
  throw new Error("AssistantAnswer copy actions should report success and failure");
}

const assistantAnswerStart = source.indexOf("function AssistantAnswer(");
const answerRefIndex = source.indexOf("<div ref={answerRef}>", assistantAnswerStart);
const copyActionIndex = source.indexOf('content="复制"', assistantAnswerStart);

if (answerRefIndex === -1 || copyActionIndex === -1 || copyActionIndex < answerRefIndex) {
  throw new Error("AssistantAnswer copy actions should render below the final answer content");
}

if (!source.includes('className="mt-1.5 flex justify-start gap-1.5"')) {
  throw new Error("AssistantAnswer copy actions should render on the lower-left side");
}

if (source.includes('className="mt-1.5 flex justify-end gap-1.5"')) {
  throw new Error("AssistantAnswer copy actions should not be right-aligned");
}

if (messageFeedSource.includes("<MarkdownText")) {
  throw new Error("Assistant final answers should not bypass AssistantAnswer");
}
