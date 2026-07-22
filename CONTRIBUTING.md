# 贡献指南 · Contributing

感谢你对 Silicon Worker（硅基动力）的关注！本文件说明如何参与，以及贡献者授权（CLA）的约定。

> English speakers: an English summary follows each section.

---

## 许可与商业化背景 · License context

本项目采用 **[PolyForm Noncommercial License 1.0.0](LICENSE)**：源码可见、仅限非商业用途，作者保留对本项目进行商业化的全部权利。为了让作者能够**在未来对整个项目（含社区贡献）进行商业授权**，所有贡献都需要遵守下面的贡献者许可约定（CLA）。

> This project uses the PolyForm Noncommercial License 1.0.0 (source-available, noncommercial-only). To let the author commercialize the whole project — including community contributions — in the future, all contributions are subject to the Contributor License terms below.

## 贡献者许可协议（CLA） · Contributor License Agreement

**向本仓库提交贡献（PR、patch、代码、文档等），即表示你同意：**

1. 你拥有所提交内容的著作权，或已获得充分授权提交它。
2. 你授予作者（`leecho · leecho571@gmail.com`）一份**永久、全球、非独占、免费、可转授权（sublicensable）**的许可，使作者可以在**任意许可协议下**使用、复制、修改、分发你的贡献，**包括闭源与商业授权**。
3. 你保留对自己贡献的著作权，可自行另作他用；本约定不剥夺你的任何权利，只是额外授予作者上述许可。
4. 你的贡献是你的原创，或已恰当标注了来源与其许可，且与本项目许可兼容。

**By submitting a contribution (PR, patch, code, docs, etc.) to this repository, you agree that:** you own or are authorized to submit the contribution; you grant the author (`leecho · leecho571@gmail.com`) a perpetual, worldwide, non-exclusive, royalty-free, **sublicensable** license to use, reproduce, modify, and distribute your contribution **under any license, including closed-source and commercial licenses**; you retain copyright to your own contribution; and the contribution is your original work or properly attributed and license-compatible.

如需以书面形式明确记录，可在 PR 描述中加一行：

> I have read and agree to the CONTRIBUTING.md CLA. / 我已阅读并同意 CONTRIBUTING.md 中的 CLA。

## 开发流程 · Development workflow

前端使用 React 18 + TypeScript + Vite + Tailwind，后端为 Rust + Tauri 2，数据存于本地 SQLite。开始较大的改动前，建议先开 Issue 讨论方向，保持改动聚焦、自带测试。

> Frontend: React 18 + TypeScript + Vite + Tailwind. Backend: Rust + Tauri 2 with local SQLite. For larger changes, open an issue to discuss direction first; keep changes focused and tested.

## 提交前验证 · Before you submit

```bash
# 后端 / backend
cargo test --manifest-path src-tauri/Cargo.toml

# 前端或 command binding / frontend or command bindings
npm run build
```

请保持提交聚焦、附清晰的说明与动机。

Keep PRs focused with a clear description and motivation.
