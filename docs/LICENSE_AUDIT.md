# 许可证与来源清单

核验日期：2026-10-13。项目原创 Rust / Vue / 文档代码按 MIT 发布。公开行情数据属于各数据源及其权利人；项目 MIT 许可证不重新授权第三方数据。

## 未引入的参考 SDK

| 项目 | 直接核验来源 | 处理 |
|---|---|---|
| dineshpinto/geckoterminal-rs | [MIT LICENSE](https://github.com/dineshpinto/geckoterminal-rs/blob/main/LICENSE)，[已发布 Cargo 信息](https://docs.rs/crate/geckoterminal-rs/latest/source/Cargo.toml) | 公开版本 0.2.5 的信息核验；本轮没有把 SDK 加入依赖，直接按官方 V2 JSON schema 解析，减少兼容层。未因许可证推定其维护质量或完整测试覆盖。 |
| solsonar/solana-multi-dex-decoder | [MIT LICENSE](https://github.com/solsonar/solana-multi-dex-decoder/blob/main/LICENSE) | JavaScript 参考库，没有复制实现或纳入 Rust 生产解码器。未声称其解析已在本项目验证。 |
| raydium-io/raydium-sdk-V2 | [GPL-3.0 LICENSE](https://github.com/raydium-io/raydium-sdk-V2/blob/master/LICENSE) | 没有复制、翻译或链接实现；本项目只消费聚合器公开响应中的 Raydium 标识。 |
| anza-xyz/solana-sdk、alloy-rs/alloy | 用户指定的生态参考 | 本轮不依赖完整 SDK，仅用 reqwest 发送官方只读 JSON-RPC；无签名器、钱包或私钥组件。 |

## 实际依赖审计方法

Rust 版本由 `backend/Cargo.lock` 锁定；前端由 `frontend/package-lock.json` 锁定。实际依赖清单应以锁定包的 Cargo metadata / npm package license 字段为准，不把参考仓库许可证当作全部传递依赖的证明。

直接 Rust 依赖使用 MIT、Apache-2.0 或双许可为主：Tokio、Axum、tower/tower-http、tracing、rusqlite、httpdate 为宽松开源组件；serde、serde_json、reqwest、chrono、clap、anyhow、bs58 等允许选择其 MIT/Apache 授权。SQLite bundled 引擎的上游说明为公有领域。构建依赖或浏览器测试工具可能引入额外许可证，需随锁文件变动重新核验。

运行发布审计时记录 `cargo metadata --locked --format-version 1` 输出的包名、版本和许可证到 `docs/evidence/provider-dependency-licenses.json`。该清单是机器生成的许可证元数据审计，不等同于法律意见；未声明或新增 copyleft 项必须人工复核后再发布。项目没有把 GPL Raydium 源码纳入 MIT 代码库。

本次已实际生成 [锁定 Rust 许可证清单](evidence/provider-dependency-licenses.json)，共 208 个包（含本项目与不同平台依赖），没有缺失许可证字段。启用 reqwest `system-proxy` 后新增 core-foundation、system-configuration、system-configuration-sys、windows-registry，四者均为 `MIT OR Apache-2.0`。唯一包含 LGPL 文本的 `r-efi 6.0.0` 提供 `MIT OR Apache-2.0 OR LGPL-2.1-or-later`，本项目选择 MIT 许可。另有 Unicode-3.0 数据、ISC、BSD、CDLA-Permissive-2.0（webpki 根证书数据）等独立通知义务；这些许可证不能仅因本项目 MIT 而被删除。此次没有发现必须按 GPL / AGPL / SSPL / BUSL 或 MPL 发布本项目原创源码的依赖。

V1.1 加入 ESLint 后，已重新从当前锁文件生成 [npm 许可证清单](evidence/provider-frontend-dependency-licenses.json)，共 234 个包：197 MIT、18 Apache-2.0、8 ISC、8 BSD-2-Clause、2 BSD-3-Clause、1 BlueOak-1.0.0；没有缺失许可证字段，包含构建与测试依赖。Rust 仍为 208 个包，清单中的本项目版本已更新至 1.1.0。这是锁文件的许可证元数据快照，更新依赖时需重新生成。

## API 与数据条款

[DEX Screener API reference](https://docs.dexscreener.com/api/reference)、[GeckoTerminal API](https://api.geckoterminal.com/docs/index.html)、[Solana public RPC](https://solana.com/docs/references/clusters)、[BNB 官方节点文档](https://docs.bnbchain.org/bnb-smart-chain/developers/json_rpc/json-rpc-endpoint/) 是接口与限制来源。开源代码许可证不覆盖第三方 API 条款。测试 fixture 仅保留少量公开响应字段用于解析回归，显式命名 recorded fixture，生产路径不引用它们，不作为行情兜底。
