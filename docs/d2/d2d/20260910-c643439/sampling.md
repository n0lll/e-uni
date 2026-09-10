# D2-D 采样预算（预先声明，不事后挑选）

- 完整 C5：40 条运行全部 `enable_probe=false`。主统计与 runs.csv 只使用 `RunLedger` 账本，不保留无界 `last_decisions`。
- 短对照：`cargo run --offline -p universe_lab --bin d2 -- <run>/short`。预先指定用例 FB01/EX02/AR02/R04-dark/MV04-unit，单体，H≤12。短对照若 `enable_probe=true`，缓存由有限 H 与单一个体限制。
- 不根据 C5 结果再导出额外决策帧。
- 冻结组正常感知、抽样、行动并支付认知费；只关闭在线读出更新。
