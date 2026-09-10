# F2 准备未解决问题

## 本轮隔离

- R08：单体 `initial_count=1`，无共享配额竞争。
- R09：`disable_reproduction=true`，无损伤，repair 仍未接入世界步进。AT02/AT03 未跑。
- Archive F05/F06：`close_batch` 同边界覆盖与 `pending_bytes` 失败不回退 **未修**。F2 不使用 Archive。
- `World::from_state` 仍无校验；权威加载入口是 `from_state_checked` / `into_world_checked` / `ExperimentCheckpoint`。
- observer/narrator 未在本轮验收。

## 正式 F2 前

- 学习门禁仍以开发配对结果为准，不得把 10001–10012 或 101–120 当正式评估集。
- 若负对照误判或 C3 不可达，不得冻结正式 L/W/H。
- 本次开发配对 bootstrap 含 0，学习门禁未过；候选 L/W/H 仅因控制条件成立而可提交审查，不是正式通过。
