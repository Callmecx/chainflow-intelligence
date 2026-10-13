# GitHub 发布与免费快照部署

公开仓库为 [Callmecx/chainflow-intelligence](https://github.com/Callmecx/chainflow-intelligence)，公开站点为 [GitHub Pages](https://callmecx.github.io/chainflow-intelligence/)。本文描述 V1.1 当前发布流程；当前发布核对见 [V1_1_ACCEPTANCE.md](V1_1_ACCEPTANCE.md)，V1 历史记录见 [ACCEPTANCE.md](ACCEPTANCE.md)。新版本的提交、CI 和 Pages 状态须按该版本的实际远端证据确认，不能沿用 V1 成功状态。

## 费用与使用边界

本项目不采购 API、云服务器、数据库、域名或订阅，不配置信用卡。GitHub 公开仓库的标准 GitHub-hosted runner 运行时间免费；larger runner 计费，因此工作流只使用 `ubuntu-latest` 和 Windows 脚本测试需要的 `windows-latest` 标准 runner。Pages 构件保留 1 天、失败浏览器诊断保留 3 天；不扩容付费存储或部署远端持久数据库，仍应留意账户的构件/缓存配额。[GitHub Actions 计费](https://docs.github.com/en/billing/concepts/product-billing/github-actions)

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

下面的创建命令仅记录首次初始化流程；当前目标仓库已经存在，不应再次创建或替换。只有本地构建/测试通过且新目标确实不存在时才适用：

```powershell
gh repo create Callmecx/chainflow-intelligence --public --source . --remote origin --push
```

已存在且远端历史经确认兼容时使用正常 `git push origin main`。本文件是操作说明，命令被写在文档中不表示已经执行。

## 启用 Pages

仓库 Settings → Pages → Build and deployment → Source 选择 **GitHub Actions**。地址栏为 `https://github.com/Callmecx/chainflow-intelligence/settings/pages`。确认 Settings → Actions → General 允许本项目使用的 Actions。部署环境 `github-pages` 如需人工批准，应由拥有者在 Actions 页面处理。

CLI 也可在已有权限时通过 GitHub Pages REST API 设为 workflow 模式；是否成功必须以 API 结果确认。使用官方 `configure-pages`、`upload-pages-artifact`、`deploy-pages`，部署 job 仅申请 `pages:write` 和 `id-token:write`。[自定义 Pages 工作流](https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages)

## 工作流行为

`ci.yml` 在 main 推送、PR 或手动触发，执行 Rust fmt/clippy/test、前端 typecheck/lint/unit/build、Chromium Playwright 页面测试，以及 `windows-latest` 上的 PowerShell 离线观察脚本测试。自动化测试依赖明确标记的 fixture，不把外部免费服务瞬时可用性作为确定性单测条件，也不等同于真实 Telegram 发送或自然长跑。

`pages.yml` 在 main 推送、手动触发、UTC `17,47 * * * *` 执行：构建 Rust → `scan-once` 采集真实公开数据 → 校验快照 → `VITE_MODE=PUBLIC_SNAPSHOT` 构建 Vue → 上传静态构件 → 部署 Pages。扫描失败、超时或输出异常时生成 `SOURCE_ERROR` 空快照，明确“数据更新失败 / 无最新数据”，不会替换成假代币。构建或部署本身失败时旧站点可能继续存在，页面时间戳会进入 STALE。

Vite base 固定 `/chainflow-intelligence/`；详情采用 hash 路由。`scan-once` 省略 `--db` 会创建唯一临时库；显式 `--db` 必须指向新文件，已有数据库一律拒绝。Actions 使用该次 runner 的隔离库，不跨次保存历史，也不读取本机数据。快照仅进入构件，不能每半小时提交到 Git。PUBLIC_SNAPSHOT 不启动 Telegram 发送、不公开通知配置、私人关注名单、数据库或运行 PID。无需为 Pages 设置 Telegram Secrets。

## 求职作品封版范围

V1.1 以 PORTFOLIO_READY 稳定封版：核心本地功能、自动化检查、真实短 Canary、CI 与公开站点分别保存证据。Telegram 真实投递及 6h/24h 自然长跑目前为 **PENDING**，按本次范围不阻塞发布，也不能写成已验证。作品讲解和可复查测试入口见 [PORTFOLIO_GUIDE.md](PORTFOLIO_GUIDE.md)；本文件不代替最终提交与部署验收。

## 验证实际部署

```powershell
git rev-parse HEAD
gh api repos/Callmecx/chainflow-intelligence/commits/main --jq .sha
gh run list --repo Callmecx/chainflow-intelligence --limit 6
gh api repos/Callmecx/chainflow-intelligence/pages --jq .html_url
```

使用 Pages API 返回的 `html_url` 验证实际 HTTP 响应，并在真实浏览器打开首页、至少一个 token 详情、手机视图、复制完整地址、外链、数据源 UTC 时间、空数据和 STALE 状态。故障快照也可能正常返回 HTTP 200，必须继续核对 mode、时间、items、provider_errors 及页面错误提示。仅有成功 push 不代表 CI 成功；部署工作流成功也不能替代页面 HTTP/交互验证。失败记录应附实际 run URL 和原因。
