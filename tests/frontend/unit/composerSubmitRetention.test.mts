import { readFileSync } from "node:fs";

const composerSource = readFileSync("src/components/session/Composer.tsx", "utf8");
const sessionPageSource = readFileSync("src/pages/session/SessionPage.tsx", "utf8");
const draftPageSource = readFileSync("src/pages/session/SessionDraftPage.tsx", "utf8");

if (!composerSource.includes("onSubmit: (text: string) => Promise<void>")) {
  throw new Error("Composer should model submit as async so it can clear input only after success");
}

const doSubmitStart = composerSource.indexOf("const doSubmit = async");
if (doSubmitStart < 0) {
  throw new Error("Composer doSubmit should be async");
}
const doSubmitEnd = composerSource.indexOf("return (", doSubmitStart);
const doSubmitBody = composerSource.slice(doSubmitStart, doSubmitEnd);

const awaitSubmitIndex = doSubmitBody.indexOf("await onSubmit(full)");
const clearIndex = doSubmitBody.indexOf("inputRef.current?.clear()");
if (awaitSubmitIndex < 0) {
  throw new Error("Composer should await parent submit before clearing input");
}
if (clearIndex < 0 || clearIndex < awaitSubmitIndex) {
  throw new Error("Composer should clear input only after parent submit succeeds");
}

const suggestionsStart = composerSource.indexOf("{suggestions && suggestions.length > 0");
const suggestionsEnd = composerSource.indexOf("{/* 主输入容器", suggestionsStart);
const suggestionsBody = composerSource.slice(suggestionsStart, suggestionsEnd);
if (!suggestionsBody.includes("inputRef.current?.setText(s)")) {
  throw new Error("Composer suggestions should fill the input for user editing");
}
if (suggestionsBody.includes("doSubmit(")) {
  throw new Error("Composer suggestions should not immediately submit on click");
}

if (!sessionPageSource.includes("const onSubmit = async (text: string): Promise<void>")) {
  throw new Error("SessionPage onSubmit should expose async success/failure to Composer");
}
if (!sessionPageSource.includes('notify.error("发送失败：')) {
  throw new Error("SessionPage should notify the user when submit fails and input is retained");
}
if (!sessionPageSource.includes("throw err;")) {
  throw new Error("SessionPage should rethrow submit failures so Composer does not clear input");
}

if (!draftPageSource.includes("const onSubmit = async (text: string): Promise<void>")) {
  throw new Error("SessionDraftPage onSubmit should expose async success/failure to Composer");
}
if (!draftPageSource.includes("throw err;")) {
  throw new Error("SessionDraftPage should rethrow submit failures so Composer does not clear input");
}
