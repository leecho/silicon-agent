---
name: create-expert
version: 1.0.0
description: Guides creating a standalone expert (an assistant role) on this platform. Use when the user wants to create, add, design, or author an expert / assistant role, or asks how an expert should be designed.
description_zh: 引导创建一个散装专家（助手角色）。当用户想创建、新增、设计或制作一个专家/助手角色，或询问专家该怎么设计时使用。
---

# 创建专家（助手角色）

本技能引导你与用户一起敲定一个**散装专家**，最后调用 `install_expert` 工具把它登记到平台。
专家 = 一个有明确职责、人设与产出格式的助手角色；创建后可在会话里选作对话身份、被主助手派发、或编入团队。

## 第一步：问清需求

在设计前，先问用户：

1. **干什么活**：这个专家要帮用户完成什么类型的任务？
2. **怎么干**：有哪些行事准则、硬约束（只读 / 数据须有出处 / 范围限制等）？
3. **产出什么格式**：期望的输出结构或模板（如「结论 / 证据 / 风险 / 建议」）？

若已有对话上下文可直接推断，就不必逐条追问。

## 第二步：设计角色

- **system_prompt**（角色设定正文，最关键）：写清三段——① 身份与目标（它是谁、要交付什么）；② 行事准则与硬约束；③ 产出格式。这段会成为该专家的人设。
- **tools**（可用工具白名单）：从任务实际需要出发挑**最小必要集**（如检索类给 `web_search`/`web_fetch`/`read_file`；代码定位类给 `read_file`/`grep` 等只读工具）。留空则默认开放全部工具。
- **model**：模型档位，`main`（主力，默认）或 `aux`（辅助）。
- **display_name** / **profession**：可选显示名与头衔。
- **quick_prompts**：几条用户引导语（示范怎么用它，如「帮我分析这家公司的财报」）。

## 第三步：登记

设计敲定后，**调用 `install_expert` 工具**真正创建：把上面各字段作为参数传入（`name`、`description`、`system_prompt` 为必填）。
只在对话里描述方案**不算创建**——一定要发起 `install_expert` 工具调用。
登记会请求用户确认（写持久全局状态）；调用前用一句话告诉用户你要登记什么。
完成后它会出现在「专家」列表，可在会话选用或编入团队。
