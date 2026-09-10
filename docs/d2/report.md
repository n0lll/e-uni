# D2 修复报告

> D2-D 收口（修复后完整 C5）见 [`d2d/20260910-c643439/report.md`](d2d/20260910-c643439/report.md)。本文件仍是 D2 提交前短对照，身份保持 `4f237b0` dirty，没有改成新提交号。

源码提交 `4f237b0e4adddf3df42ebf782b4367e9ac2ef3ab` dirty=true source_digest=`ec9d8b166366e0b159ed4cd023e69d745beac6e9949ba49f5909473329708df3`

本报告由 `docs/d2/` 结构化结果生成。`docs/d1/` 与 `docs/batches/G5.md` 保持原件。

**状态：所列短对照均记录为 ok；这不是学习门禁通过，也不是 F2 正式跑数。**

## 对照

- FB01：ok=true error=— decisions=2 
- EX02：ok=true error=— a=f6842abe678518749d33b52797d706b0c495db5785c513418c51086163b5db81 b=33dafe8bd64f757259709bd36d6ccc7e367a914a3159259c24cb339595d63126 c=dbd9c79cb30c21584300855b908b5036fa3d8da6f59e19085f1987e329febdc9
- AR02：ok=true error=— pack=9b973fe33f1200b3d6cc8347b433c6d1566ac7b8e4a6b7ece67f45c43bf23550
- R04-dark：ok=true error=— keep_q=1800000 quench_u0=20000000 keep_u0=21800000
- MV04-unit：ok=true error=— paid=600

## 实际规则哈希

- 128×128 基础 RuleSpec：`6e27f665f00d508e8a49b0a9fb6409163fa4c7970ec3d58755f13b90ae180a9b`
- 本轮 16×16 实验 RuleSpec（FB01）：`34ecf5b25d8952aade89636bb8bfd83afc3aa34e44a1cb10e932578cd96931d5`

学习率、奖励系数与网络规模未改。未推进 E/S/配对。

F2 的 200-tick 等待只限制指标窗口，不推迟学习/冻结分支时刻。正式种子冻结前不跑正式 F2。

## 未解决问题

- R08 P2: 共享配额平局仍固定 priority=0，未接入 QuotaTie；单体 F2 无竞争，可隔离。
- R09 P2: repair 反应未接入世界步进，属 B 版缺失而非 E/S。F2 默认满结构、无损伤、无出生，可隔离；完整 B 版须补世界级 repair。出生当步仍累计新生体反馈，繁殖验收前处理。
- R10 P2: validate 未拒绝 q_loss_divisor=0 / substeps=0 等；部分入哈希参数仍有硬编码路径。参数扫描前修复。
- R11 P2: enable_probe 时 last_decisions 无界且每步深拷贝。长时间探针运行前须加采样预算。
- AT02/AT03: 胚体构造与出生 ID 分配故障注入列入繁殖验收批次；本轮单体关繁殖 F2 延期。
- AR03 档案事件事务已补幂等主键；长期档案产品其余能力不以通过论。
