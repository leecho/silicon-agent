import { readFileSync } from "node:fs";
import ts from "typescript";

const source = readFileSync("src/components/session/toolNarrative.ts", "utf8");
const compiled = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.ESNext,
    target: ts.ScriptTarget.ES2022,
  },
});
const mod = await import(
  `data:text/javascript;base64,${Buffer.from(compiled.outputText).toString("base64")}`
);

const { getToolNarrative, toolNarrative } = mod as {
  getToolNarrative: (toolName: string) => string;
  toolNarrative: (toolName: string, status: string, inputJson: string) => string;
};

function expectEqual(actual: string, expected: string, label: string) {
  if (actual !== expected) {
    throw new Error(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

expectEqual(getToolNarrative("remember"), "记录记忆", "remember display name");
expectEqual(getToolNarrative("collect_agents"), "收取智能体结论", "collect_agents display name");
expectEqual(getToolNarrative("update_tasks"), "更新任务台账", "update_tasks display name");

expectEqual(
  toolNarrative("remember", "done", JSON.stringify({ content: "用户偏好用中文沟通" })),
  "已记录记忆 用户偏好用中文沟通",
  "remember content fragment",
);

expectEqual(
  toolNarrative("create_team", "running", JSON.stringify({ name: "research", display_name: "研究团队" })),
  "正在创建团队... 研究团队",
  "create_team display fragment",
);

expectEqual(
  toolNarrative("ask_user", "generating", JSON.stringify({ questions: [{ question: "要使用哪个数据源？" }] })),
  "正在准备向用户提问... 要使用哪个数据源？",
  "ask_user question fragment",
);

expectEqual(
  toolNarrative("update_tasks", "running", JSON.stringify({ goal: "完成研报", tasks: [{ title: "调研" }] })),
  "正在更新任务台账... 完成研报",
  "update_tasks goal fragment",
);

expectEqual(
  toolNarrative("collect_agents", "running", JSON.stringify({ handles: ["agent-a", "agent-b"] })),
  "正在收取智能体结论... 2 个智能体",
  "collect_agents handles fragment",
);
