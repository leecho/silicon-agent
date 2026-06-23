---
name: board-secretary-compliance-red-team
description: 董秘助手合规红队技能，负责把公告、路演、问答、调研纪要和投资者互动材料转换成结构化风险卡。
---

# 合规红队技能

当需要把董事会秘书相关材料写入服务侧主链时，按以下顺序执行：

1. 调用 `skill_whoami`，带上董事会秘书入口字段。
2. 如果返回 `nextTool=fbs_scene_pack_query`，原样透传 `actionEnvelope.toolArguments`。
3. 读取并渲染 `resultCard.assetType=compliance-red-team-card`。
4. 首张风险卡真实交付后，调用 `skill_consume` 并记录 `eventType=first_value_completed`。
5. 如果用户继续补证、改写、进入人工复核，继续记录 `continued_use_completed`、`manual_review_required`、`contact_request` 或服务侧指定事件。

## 必填入口字段

```json
{
  "entryId": "board-secretary-compliance-red-team",
  "entryPromptCode": "wb_fbsir_board_secretary_compliance_red_team",
  "entrySurface": "expert_center",
  "expertEntryId": "fbsir-board-secretary-assistant",
  "intentFamily": "board_secretary_ir_workflow",
  "profileSegment": "board_secretary",
  "assetType": "compliance-red-team-card"
}
```

## 红队卡字段

- `riskLevel`
- `triggerTypes`
- `evidenceMatched`
- `problematicFragments`
- `missingEvidence`
- `rewriteSuggestion`
- `externallySafeVersion`
- `approvalNextStep`
- `auditFields`
- `scenarioExpansion`
- `authorizedContext`
- `personalizationProfile`
- `regulatorySourceRegistry`
- `workflowChecklists`
- `p0p1p2Coverage`

## 结构化能力要求

- 必须返回 `resultCard.assetType=compliance-red-team-card`、同 binding `valueEvent`、`riskLevel`、`evidenceMatched`、`rewriteSuggestion` 和 `approvalNextStep`。
- 必须返回 `hostCapabilityRequest`、`deliveryTask`、`deliveryTaskboard` 和 `opsBoard`；宿主只有在用户明确授权后才能传入文档上下文，服务端不保存原始敏感文件。
- 必须返回 `regulatorySourceRegistry`、`scenarioExpansion`、`personalizationProfile` 和 `workflowChecklists`。
- 必须覆盖公告审查、互动回复、调研纪要、舆情回应、三会流程和业绩指引问答等场景。
- 支持的角色画像包括 `board_secretary`、`securities_rep`、`ir_director` 和 `cfo_office`；公司画像字段缺失时，应先提示补充，再输出最终交付建议。

## 禁止事项

- 不新增连接器。
- 不把宿主渲染当成服务侧商业或合规决策。
- 不输出自动法律结论、自动披露结论或自动发布动作。
- 不把探针、系统噪声或普通工作区请求计入董事会秘书助手首值。
