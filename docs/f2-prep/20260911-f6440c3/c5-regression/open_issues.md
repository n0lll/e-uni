# D2-D 未解决问题

## 本轮实际隔离

- R08：`initial_count=1`，单体，无共享资源竞争；未验证多体 QuotaTie。
- R09：`disable_reproduction=true`，未注入损伤；repair 仍未接入世界步进。出生当步反馈未测。AT02/AT03 未跑。
- R10：本轮配置 `q_loss_divisor=16`、`substeps=4`、dt=1/10、`decision_every_ticks=5`。EX03 仅覆盖非空 future_inputs，不能宣称零除数等全部被拒绝。
- R11：完整 C5 `enable_probe=false`，不把无界 `last_decisions` 用于长运行。内核无界探针 API 未改；有界探针仍是后续缺陷。短对照见 `sampling.md`。
- 档案：本轮 C5 不使用档案恢复。现有测试覆盖 S0、事件幂等、rules mismatch；换灯边界恢复与决策/非决策边界对照未作为本轮 C5 路径验收。不得写「档案全部通过」。
- 本轮 C5 未使用非空 future_inputs；该路径保持拒绝。

## 交接清单中的原表述

- R08 P2: 共享配额平局仍固定 priority=0，未接入 QuotaTie；单体 F2 无竞争，可隔离。
- R09 P2: repair 反应未接入世界步进，属 B 版缺失而非 E/S。F2 默认满结构、无损伤、无出生，可隔离；完整 B 版须补世界级 repair。出生当步仍累计新生体反馈，繁殖验收前处理。
- R10 P2: validate 未拒绝 q_loss_divisor=0 / substeps=0 等；部分入哈希参数仍有硬编码路径。参数扫描前修复。
- R11 P2: enable_probe 时 last_decisions 无界且每步深拷贝。长时间探针运行前须加采样预算。
- AT02/AT03: 胚体构造与出生 ID 分配故障注入列入繁殖验收批次；本轮单体关繁殖 F2 延期。
- AR03 档案事件事务已补幂等主键；长期档案产品其余能力不以通过论。

## F2 前（本轮不执行）

- 公开共同初态含脑、独立生成器、T_recover、留 Q 关输入、负对照、C3/C4 可达性，再冻 L/W/H。
- 学习/冻结在换灯后首次神经行动前分支；200 ticks 只限制指标窗口。
- 正式评估种子不得参与初态/指标/超参数选择。101–120 不是未来正式评估集。
