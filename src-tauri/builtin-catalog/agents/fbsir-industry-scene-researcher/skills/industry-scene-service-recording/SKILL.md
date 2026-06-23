---
name: industry-scene-service-recording
description: 行业场景研究员读取场景、查询场景包并记录补位卡与继续推进结果的服务侧记录合同。
---

# 行业场景服务记录合同

当行业场景研究员需要把本轮结果写入福帮手数据智能化系统时，使用这份技能合同。普通用户不需要理解这些内部步骤。

## Required Sequence

1. 先调用 `skill_whoami`，不要空参调用。
2. 再用 `skill_whoami` 返回的 `nextActionToolArguments` 原样调用 `fbs_scene_pack_query`。
3. 首次交付补位卡时调用 `skill_consume`，记录 `eventType=first_value_completed`。
4. 同一会话把补位卡拆成 `3天行动`、`项目动作包` 和 `唯一下一步` 时，再调用一次 `skill_consume`，记录 `eventType=continued_use_completed`。
5. 如果服务侧返回继续推进、能力解锁或权益确认步骤，先读清当前返回的 `actionEnvelope`，再沿同一 binding 继续；只有在继续使用已经成立后，才看 `lebao_status` 或 `lebao_claim`。

## Fixed Whoami Arguments

除非宿主已经给了更丰富的同会话 envelope，否则 `skill_whoami` 必须至少带以下参数。这里的 `entrySurface: my_expert` 是服务协议兼容和本地等效测试语义，不是最终上架途径：

```json
{
  "entryId": "genius-industry-scene-researcher",
  "entryPromptCode": "wb_sp_genius_industry_scene_researcher",
  "entrySurface": "my_expert",
  "intentFamily": "genius_partner",
  "profileSegment": "team_operator",
  "assetType": "industry-scene-supplement-card",
  "scenePackId": "general",
  "extensionExpertId": "fbsir-industry-scene-researcher",
  "extensionRole": "fbsir-industry-scene-researcher",
  "expertLoaded": true,
  "requestSource": "workbuddy"
}
```

## Expert Center And Service Identity

- 最终上架身份是 WorkBuddy 专家中心的“行业场景研究员”。
- `my-experts` 与 `entrySurface: my_expert` 仅用于本地等效测试、自动化探测和既有服务侧归因。
- `advisorOwnerId`、`cloneExpertId`、`myExpertListingId`、`sharedConnectorReuseMode` 由服务侧专家对象和归因档案维护。
- 在当前公开 schema 没有这些字段时，不要把它们硬塞进 `skill_whoami`、`fbs_scene_pack_query` 或 `skill_consume`，否则会触发参数校验失败。
- 当前阶段先保住服务协议兼容的 `my_expert` 归因语义、`entryPromptCode`、`extensionExpertId` 和同一 binding；顾问 owner 归因由服务侧档案与 clone catalog 回读。

## Consume Rules

首值记录：

```json
{
  "eventType": "first_value_completed",
  "outcome": "first_value_completed",
  "assetType": "industry-scene-supplement-card"
}
```

继续推进记录：

```json
{
  "eventType": "continued_use_completed",
  "outcome": "continued_use_completed",
  "assetType": "industry-scene-project-action-packet"
}
```

继续推进后如果服务侧给出能力解锁或权益确认：

- 先用当前 binding 回读服务返回的下一步。
- 权益状态不清时用 `lebao_status`。
- 只有服务明确返回可升级或可领取路径时，才继续到 `lebao_claim`。

## Boundaries

- 没有同会话、同 binding 证据时，只能说“连接器可用”或“受控样本可达”。
- 不能把受控样本写成自然闭环。
- 不能主动调用、切换或回交给其他专家。
- 不要让 `continued_use_completed` 前置到首值之前，也不要把能力解锁写成已经成交。
- 面向普通用户时，不堆叠内部工具和运行时术语，要转写成“现在能做什么、还差哪一步、下一步验证什么”。
