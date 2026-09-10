# D2-D 报告

源码 HEAD `f6440c39c42bc544466199fcb405092e0af7a0bd` git_dirty=true execution_source_differs_from_head=true source_digest=`1f95b33591ee959a59f2b4e3b06cd81183226c7e7028f74ec817ab8c7ba91076` algo=`eu-source-digest-v1`

本报告由本目录结构化结果生成。`docs/d1/` 与 `docs/batches/G5.md` 原件未改。既有 `docs/d2/` 短结果保留提交前身份，不是把 commit 改成当前 HEAD 的新运行。

**状态：40 条 C5 记录完整且无工程错误。这不是学习门禁通过，也不是 F2。**

## 协议

- 命令：`cargo run --release --offline -p universe_lab --bin d2d -- docs/f2-prep/20260911-f6440c3/c5-regression`
- profile：release
- rustc：rustc 1.92.0 (ded5c06cf 2025-12-08)
- 耗时：7229 ms
- experiment_hash：`32fd194a5602730c99c4fc30c788d57f0ffd7cf328c54beefc66dcf4116b6004`
- 实际 16×16 RuleSpec：`34ecf5b25d8952aade89636bb8bfd83afc3aa34e44a1cb10e932578cd96931d5`（D1 runs.csv 曾写入 128×128 基线 `6e27f665f00d508e8a49b0a9fb6409163fa4c7970ec3d58755f13b90ae180a9b`，这是公开的记录差异，不是无说明的同配置复现）
- 换灯后 RuleSpec：`70a5f88e2ed6d3a685387d27e622d53f49b0261f08bc2c916f079d7ff17cdd7a`（`apply_lighting`，与 state.rules_hash 对齐）
- 地图 16×16、种子 101–120、learn/freeze、20000 ticks、tick=10000 换灯、关繁殖、`enable_probe=false`。
- 主指标：后半程代谢 E 的逐 seed `学习−冻结`；bootstrap 种子 20260910、2000 次；重采样单位是完整 seed 对。
- 学习率、奖励、网络未改。未引入 F2 低库存或 L/W。死亡样本全部保留。

## 完整性

- 行数 40（应为 40）；配对 true；冻结组更新全零 true；探针全关 true；初态成对 true；摘要未变 true
- P 会计残差：40 条可计算，其中 40 条为 0。残差定义见 statistics.json。非零时不改账本凑零；诊断 P 不替代 M/U 守恒。常见来源包括容量上限、N 库存变化、未列入公式的耗散/修复项、以及死亡当步账户语义。

## C5 主比较

后半程代谢 E（学习−冻结）配对均值 57818.0，bootstrap 95% [-3703555.2, 3958648.4]。

结论：尚未观察到可靠收益

换灯存活：学习 4 / 冻结 2；期末存活：学习 1 / 冻结 1（各 20）。

历史 D1 C5（`docs/d1/report.md`，不是本轮目标）：均值 +195084.0 μE，95% [-3581658.0, +4097096.8]。

## 逐 seed 差值

| seed | learn−freeze μE | learn 换灯存活 | freeze 换灯存活 |
|---|---:|---|---|
| 101 | 0.0 | false | false |
| 102 | 0.0 | false | false |
| 103 | 0.0 | false | false |
| 104 | 0.0 | false | false |
| 105 | 26005536.0 | true | false |
| 106 | 0.0 | false | false |
| 107 | 0.0 | false | false |
| 108 | 0.0 | false | false |
| 109 | 0.0 | false | false |
| 110 | 15216.0 | true | false |
| 111 | 0.0 | false | false |
| 112 | 8802800.0 | true | false |
| 113 | 0.0 | false | false |
| 114 | 0.0 | false | false |
| 115 | -25952920.0 | false | true |
| 116 | -11097848.0 | false | true |
| 117 | 3383576.0 | true | false |
| 118 | 0.0 | false | false |
| 119 | 0.0 | false | false |
| 120 | 0.0 | false | false |

## 未解决问题

见 `open_issues.md`。F2 项目本轮未执行。
