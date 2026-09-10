# 工程规则（电子宇宙 / e-uni）

- 测试入口：`cargo test --offline --workspace --exclude observer --exclude narrator`。Rust 1.92.0 与 `Cargo.lock`，不为方便升级依赖。
- 历史证据：保留 `docs/d1/`、`docs/batches/G5.md`、既有 `docs/d2/`。新结果写入唯一新目录，不用 latest 覆盖。
- 确定性与守恒：规范世界状态走 canonical 编码与 `state_hash`。诊断/探针不得改变轨迹。
- 禁止未经批准调参：不改学习率、奖励系数、网络规模、先天策略偏置、痕迹、熵、基线约束；不靠屏蔽静止动作或加先天寻路让神经组过门禁。
- 实验禁止补资源、复活、删死亡样本、刷种子直到显著。数值错误、实验错误与死亡必须分开。
- 提交与推送：不自动 `git commit` / `push`。需要提交时由维护者明确要求。
- F2 使用 `from_state_checked` / 实验 checkpoint；`World::from_state` 无加载保证。Archive 不是 F2 权威路径。
