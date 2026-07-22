# 发版流程 (Releasing)

源码在本私有仓库;安装包发布到公开仓库 **`leecho/silicon-worker-release`** 的 Release,
营销站 <https://silicower.com> 的下载按钮指向该 Release 的 `latest` 直链。

---

## 一次性准备（已配好则跳过）

- **GitHub App**（如 `silicon-worker-release-bot`）：Repository permission `Contents: Read and write`，
  安装到 `leecho/silicon-worker-release`。
- 本私有仓 → Settings → Secrets and variables → Actions：
  - Variable：`RELEASE_APP_ID`
  - Secret：`RELEASE_APP_PRIVATE_KEY`（App 私钥 .pem 全文）
- 公开仓 `leecho/silicon-worker-release` 存在 `main` 分支。

> 私钥泄露或轮换：App → Private keys → 删旧、Generate 新 → 更新 `RELEASE_APP_PRIVATE_KEY`。

## 发一个版本（手动触发，版本号自动算）

1. 打开 **Actions → Release → Run workflow**。
2. **bump** 选递增级别：
   - `patch`：修订（1.2.3 → 1.2.4）
   - `minor`：功能（1.2.3 → 1.3.0）
   - `major`：重大/破坏性（1.2.3 → 2.0.0）
3. Run。版本号会在**公开仓最新 Release** 的基础上自动 +1（首次发布从 `0.0.1 / 0.1.0 / 1.0.0` 起）。

> 不用打 tag、不用手改任何版本文件。

## 自动发生了什么

1. **meta**：读公开仓最新 Release 版本，按 bump 算出下一个版本号。
2. **build**（矩阵，三平台）：把版本号写进 `tauri.conf.json` / `package.json`（仅 CI 内），
   macOS Apple 芯片 / macOS Intel / Windows 各自构建，产物**重命名为固定名**。
3. **publish**：
   - 用私有源码仓的 commit 生成 changelog（区间 = 上一个发布 tag → 现在）；
   - 在私有仓打一个轻量 tag `vX.Y.Z`（发布记录 + 下次 changelog 的起点）；
   - 用 GitHub App 短期 token 把三平台产物 + changelog 发布到
     `leecho/silicon-worker-release` 的 Release。

## 产物固定名（站点直链依赖，请勿改动）

```
SiliconWorker-macos-arm64.dmg
SiliconWorker-macos-x64.dmg
SiliconWorker-windows-x64.exe
```

## 验证清单

- [ ] 公开仓 Releases 出现新版本，含上面 3 个安装包
- [ ] 安装包内部版本 = 新版本号（About / 文件属性）
- [ ] 站点下载页三平台均可下载

## 站点下载链接（固定，不随版本变）

```
https://github.com/leecho/silicon-worker-release/releases/latest/download/SiliconWorker-macos-arm64.dmg
https://github.com/leecho/silicon-worker-release/releases/latest/download/SiliconWorker-macos-x64.dmg
https://github.com/leecho/silicon-worker-release/releases/latest/download/SiliconWorker-windows-x64.exe
```

→ **发新版本无需改站点。**

## 备注

- 版本号的「真实来源」是公开仓的最新 Release；仓库里 `tauri.conf.json` 的 `version` 平时停在占位值即可，
  发版时由 CI 临时覆盖。
- 发布说明由**私有源码仓的 commit** 生成（上个发布 tag → 现在的提交列表）。每次发版会在私有仓打 `vX.Y.Z` tag
  作为下次 changelog 的起点——所以私有仓会逐渐积累发布 tag，这是预期行为。
- 想要更规整的分组 changelog（Features / Fixes 等），后续可接入 git-cliff 或按 Conventional Commits 生成。
