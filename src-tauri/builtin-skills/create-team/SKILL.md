---
name: create-team
version: 1.0.0
description: Guides creating a team (a lead plus members) on this platform. Use when the user wants to create, add, design, or author a team / group of expert roles, or asks how a team should be organized.
description_zh: 引导创建一个团队（主理人+成员）。当用户想创建、新增、设计或制作一个团队/角色编组，或询问团队该怎么编排时使用。
---

# 创建团队（主理人 + 成员）

本技能引导你与用户一起敲定一个**团队**，最后调用 `install_team` 工具把它登记到平台。
团队 = 一名主理人（lead）+ 若干成员（members）。主理人/成员都在本次登记调用里**现场定义**（成为该团队的私有专家）。创建后团队出现在「团队」列表、可在会话激活。

## 第一步：问清需求

先问用户：

1. **做什么任务**：这个团队要协作完成什么？
2. **需要哪些角色分工**：拆成哪些成员各司其职？

## 第二步：设计编组

- **主理人（lead，可选）**：负责统筹、决定怎么把活分给谁——它**不直接干活、不进可派发名单**，其 `system_prompt` 作为团队协作说明（SOP）。
- **成员（members，至少一名）**：实际干活、可被主助手派发。每个成员写清：
  - `name`（唯一标识，可中文）、`description`（一句话职责）；
  - `system_prompt`（身份 / 行事准则 / 产出格式三段）；
  - `tools`（最小必要集，留空默认全开）、`model`（`main` 默认 / `aux`）、可选 `display_name`/`profession`。
- **quick_prompts**：几条开场引导语（示范怎么用这个团队，如「帮我做一份这周的竞品分析」）。

## 第三步：登记

设计敲定后，**调用 `install_team` 工具**真正创建：`name`（团队标识，建议英文）、`display_name`、`description`、`lead`（可选）、`members`（至少一名）、`quick_prompts` 作为参数传入（`name`、`members` 为必填）。
只描述方案不算创建。登记会请求用户确认；调用前用一句话说明你要登记什么。
完成后团队出现在「团队」列表、可在会话激活。
