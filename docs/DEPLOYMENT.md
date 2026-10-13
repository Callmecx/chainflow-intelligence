# GitHub 发布与免费快照部署

目标为公开仓库 `Callmecx/chainflow-intelligence`。是否实际创建、推送、CI 通过和 Pages 可访问，以 [ACCEPTANCE.md](ACCEPTANCE.md) 的远端证据为准。

## 费用与使用边界

本项目不采购 API、云服务器、数据库、域名或订阅，不配置信用卡。GitHub 公开仓库的标准 GitHub-hosted runner 运行时间通常免费；larger runner 始终计费，因此工作流仅使用 `ubuntu-latest` 标准 runner。构件仅短期保留，不使用付费存储或远端持久数据库。[GitHub Actions 计费](https://docs.github.com/en/billing/concepts/product-billing/github-actions)

Pages 是静态托管，没有 Rust/Tokio 常驻进程。页面定位为非商业开源演示/研究快照，不用于商业交易或商业 SaaS。[Pages 使用限制](https://docs.github.com/en/pages/getting-started-with-github-pages/github-pages-limits)

定时目标为每小时 UTC 第 17 / 47 分，即北京时间每小时第 17 / 47 分。调度可能延迟或丢弃；公开仓库无活动 60 天后可能停用定时工作流。不能承诺 SLA 或 24 小时连续采集。[调度规则](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#schedule)

## 发布前核查

```powershell
gh auth status
gh api user --jq .login
gh repo view Callmecx/chainflow-intelligence
git status --short
git remote -v
```

未登录时由仓库拥有者在终端运行 `gh auth login` 并完成浏览器验证。不要把 PAT、密码或私钥发给协作者。已存在目标仓库时先 fetch 比较历史并安全合并，不覆盖、不强推。发布前确认 `.env`、数据库、日志、`.runtime`、`.tools`、用户记录均未跟踪。

仅在本地构建和测试通过且目标仓库不存在时：

```powershell
gh repo create Callmecx/chainflow-intelligence --public --source . --remote origin --push
```

已存在且远端历史经确认兼容时使用正常 `git push origin main`。本文件是操作说明，命令被写在文档中不表示已经执行。

## 启用 Pages

仓库 Settings → Pages → Build and deployment → Source 选择 **GitHub Actions**。地址栏为 `https://github.com/Callmecx/chainflow-intelligence/settings/pages`。确认 Settings → Actions → General 允许本项目使用的 Actions。部署环境 `github-pages` 如需人工批准，应由拥有者在 Actions 页面处理。

CLI 也可在已有权限时通过 GitHub Pages REST API 设为 workflow 模式；是否成功必须以 API 结果确认。使用官方 `configure-pages`、`upload-pages-artifact`、`deploy-pages`，部署 job 仅申请 `pages:write` 和 `id-token:write`。[自定义 Pages 工作流](https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages)

## 工作流行为

`ci.yml` 在 main 推送、PR 或手动触发，执行 Rust fmt/clippy/test、前端 typecheck/unit/build，以及 Chromium Playwright 页面测试。测试依赖 fixture，不把外部免费服务瞬时可用性作为确定性单测条件。

`pages.yml` 在 main 推送、手动触发、UTC `17,47 * * * *` 执行：构建 Rust → `scan-once` 采集真实公开数据 → 校验快照 → `VITE_MODE=PUBLIC_SNAPSHOT` 构建 Vue → 上传静态构件 → 部署 Pages。扫描失败、超时或输出异常时生成 `SOURCE_ERROR` 空快照，明确“数据更新失败 / 无最新数据”，不会替换成假代币。构建或部署本身失败时旧站点可能继续存在，页面时间戳会进入 STALE。

Vite base 固定 `/chainflow-intelligence/`；详情采用 hash 路由。每次扫描使用临时 SQLite，不跨次保存历史。快照仅进入构件，不能每半小时提交到 Git。所有公开资产必须无敏感凭证。

## 验证实际部署

```powershell
git rev-parse HEAD
gh api repos/Callmecx/chainflow-intelligence/commits/main --jq .sha
gh run list --repo Callmecx/chainflow-intelligence --limit 6
gh api repos/Callmecx/chainflow-intelligence/pages --jq .html_url
```

使用 Pages API 返回的 `html_url` 验证实际 HTTP 响应，并在真实浏览器打开首页、至少一个 token 详情、手机视图、复制完整地址、外链、数据源 UTC 时间、空数据和 STALE 状态。仅有成功 push 不代表 CI 成功；只有部署工作流成功也不能替代页面 HTTP/交互验证。失败记录应附实际 run URL 和原因。
