// T70：排队条组件须渲染排队项并提供逐条取消（静态源检查，沿用本仓库前端测试约定）。
import { readFileSync } from "node:fs";

const src = readFileSync("src/components/session/QueuedMessages.tsx", "utf8");
if (!src.includes("queued")) {
  throw new Error("QueuedMessages 应基于 queued 状态过滤排队项");
}
if (!src.includes("onCancel")) {
  throw new Error("QueuedMessages 应暴露 onCancel 逐条取消回调");
}

const page = readFileSync("src/pages/session/SessionPage.tsx", "utf8");
if (!page.includes("listSessionQueue")) {
  throw new Error("SessionPage 应加载会话任务队列");
}
if (!page.includes("queued_tasks_updated")) {
  throw new Error("SessionPage 应订阅 queued_tasks_updated 刷新队列");
}
if (!page.includes("queuedTasks") || !page.includes("onCancelQueued")) {
  throw new Error("SessionPage 应把队列与取消回调透传给 Composer");
}

// 队列塔嵌在 Composer 内部（输入框上方），而非 SessionPage 顶层。
const composer = readFileSync("src/components/session/Composer.tsx", "utf8");
if (!composer.includes("QueuedMessages")) {
  throw new Error("Composer 应在输入框上方渲染 QueuedMessages 队列塔");
}
console.log("queuedMessagesPanel ok");
