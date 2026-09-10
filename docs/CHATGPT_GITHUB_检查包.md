# ChatGPT GitHub 插件检查包（F2 准备）

本文件把**本轮代码改了什么、测了什么、得到什么数字**收成一份，方便在 ChatGPT 网页版用 GitHub 插件对照仓库检查。更细的 CSV / 初态 / C5 复现仍在 `docs/f2-prep/20260911-f6440c3/`。

仓库：https://github.com/n0lll/e-uni  
默认分支：`master`  
本轮相对提交：`f6440c39c42bc544466199fcb405092e0af7a0bd`（D2-D）之上的 F2 准备。

**正式 F2 未执行。学习门禁未通过。不要把 10001–10012 或 101–120 当正式评估集。**

---

## 1. 建议插件先读的源码

| 问题 | 文件 | 看什么 |
|---|---|---|
| F01 非有限动作 | `crates/universe_core/src/brain.rs` | `sample_policy_masked`：未屏蔽 NaN/+∞ 报 `NumericFault::NonFinite`；合法 −∞ mask 仍消费 RNG |
| F01 整步回退 | `crates/universe_core/src/world.rs` | `World::step` 失败恢复 `state` / ledger / probe / 聚合 |
| F02 控制器哈希 | `crates/universe_lab/src/experiment.rs` | `RuntimePack::content_hash` 前缀 `RP01` + `ControlMode::encode_canonical` |
| F02/F03 加载 | `crates/universe_core/src/world.rs` | `from_state_checked` / `validate_loaded_world`；旧 `from_state` 仍无保证 |
| F04 范数 | `crates/universe_core/src/brain.rs` `apply_readout_update`；`probe.rs` `DecisionDiag` 七个 `Option<f64>` | 无更新=`None`，测得零=`Some(0)` |
| F04 有界探针 | `world.rs` | `probe_frame_limit=256`，先 `probe_aggregate.push` 再有界存帧 |
| F07 校验 | `crates/universe_types/src/rules.rs` `validate_core1` | 零除数、零子步、leak/sense_range/w_rec_l1 硬编码 |
| F08 运动开关 | `world.rs` | 先 `apply_diagnostic_control`，再 `zero_motion_only`（只清 thrust/turn） |
| F03 checkpoint | `crates/universe_lab/src/checkpoint.rs` | `EUCK` v1，灯光时间表游标，失败不重复换灯 |
| T_recover | `crates/universe_lab/src/t_recover.rs` | 窗口 `[t-W,t)`，`t>=L+W`，有符号 ΔP，应付维持/认知 |
| F2 入口 | `crates/universe_lab/src/f2.rs`、`crates/universe_lab/src/bin/f2.rs` | 仅 `generate` / `validate` / `dev` / `recompute`，无正式评估默认入口 |

回归测试：`crates/universe_core/src/d2_tests.rs`（F01/F04/F07/F08）、`crates/universe_lab/src/checkpoint.rs`（mod tests）、`t_recover.rs`（mod tests）。

Archive（F05/F06）**本轮未修、F2 未使用**。

---

## 2. 实际执行的命令与退出码

工具链：`rustc 1.92.0 (ded5c06cf 2025-12-08)`，`cargo 1.92.0`，离线、锁文件未为方便升级依赖。

| 命令 | 退出码 | 结果 |
|---|---|---|
| `cargo test --offline --workspace --exclude observer --exclude narrator` | 0 | 工作区测试通过（排除未验收的 observer/narrator） |
| `cargo test --offline -p universe_lab --lib` | 0 | 38 passed, 1 ignored（`resource_reversal_20_seeds_full`） |
| `cargo run --release --offline -p universe_lab --bin f2 -- dev docs/f2-prep/20260911-f6440c3` | 0 | 约 89 s；见第 3 节 |
| `cargo run --release --offline -p universe_lab --bin f2 -- recompute docs/f2-prep/20260911-f6440c3` | 0 | `files=72 errors=0` |
| `cargo run --release --offline -p universe_lab --bin d2d -- docs/f2-prep/20260911-f6440c3/c5-regression` | 0 | `valid_complete=true rows=40`，与 D2-D 数字一致 |

独立复算 T_recover：读 `docs/f2-prep/20260911-f6440c3/tick_metrics/*.csv` 的 `# p0=... L= W= H=` 头，按 `t_recover.rs` 规则重建首次恢复。不要只核对本文件里已经算好的数字。

---

## 3. F2 开发实验数字（种子 10001–10012，永久 development）

候选协议（第 1 组即锁定，未改学习率/奖励/网络）：

- recipe: `v1_left_body_right_light_keep_residual_q`
- 身体公开初态 `(4.5, 8.5)`，换灯到右侧后 t=0，再分学习/冻结
- L=200，W=100，H=4000（最早合法恢复 t=300）
- protocol_hash: `22bd6d4aceecd6a10f833e87833a4a333be2cae9a44327d25f6e93cf9eba2155`
- 开发运行时执行源摘要: `1f95b33591ee959a59f2b4e3b06cd81183226c7e7028f74ec817ab8c7ba91076`

主指标（看结果前锁定）：同 seed `受限时间_冻结 − 受限时间_学习`；正值=学习更快。Died / 到 H 未恢复的受限时间均为 H。统计种子 `20260911`，2000 次，分位与 D1/D2 相同。

| 对照 | 恢复 / 12 | 要点 |
|---|---|---|
| A 残留 Q（关新辐射、留 Q，静止+禁运动） | 0 | 不在 L 后假恢复；约 t=3978 死亡 |
| B 负对照（清 Q+余数，重算 M0/U0） | 0 误判 | 光合 0；约 t=3915 死亡 |
| C3 OracleGotoLight | **12** | 全部 t=300（最早合法时刻） |
| C4 ReflexQ | **12** | 全部 t=300 |
| 神经 learn | 1（10004，t=3036） | 更新约 698–799；其余多为死亡 |
| 神经 freeze | 2（10005 t=3430，10007 t=3631） | 更新全 0 |

配对 `restricted_freeze - restricted_learn`：

```
10001  0     Died / NotRecoveredByH
10002  0     Died / Died
10003  0     Died / Died
10004  +964  Recovered(t=3036) / Died
10005  -570  Died / Recovered(t=3430)
10006  0     Died / Died
10007  -369  Died / Recovered(t=3631)
10008–10012  0  Died / Died
```

- 均值 **+2.083**
- bootstrap 95% **[-173.25, +241]**（含 0）
- 工程错误行：0
- 死亡样本全部保留

**解读：** C3/C4 与负对照说明任务物理可达、指标未误判，因此 L/W/H 可作为正式 F2 的**候选公开协议**。神经配对区间含 0，**不能**称为学习门禁通过。

原始表：

- `docs/f2-prep/20260911-f6440c3/runs.csv`
- `docs/f2-prep/20260911-f6440c3/paired_differences.csv`
- `docs/f2-prep/20260911-f6440c3/statistics.json`
- `docs/f2-prep/20260911-f6440c3/report.md`

---

## 4. C5 兼容复现（不是新的学习门禁）

因本轮改了步进顺序、非有限采样与探针缓存，用原 D2-D 协议重跑 C5（种子 101–120，20k ticks，`enable_probe=false`）。

与 D2-D 审查复算 **一致**（禁止用改基准掩盖差异）：

- 40 行完整；P 残差全 0
- 后半程代谢 E（学习−冻结）均值 **+57818.0**
- 95% **[-3703555.2, +3958648.4]**
- 换灯存活：学习 4/20，冻结 2/20；期末各 1/20

产物：`docs/f2-prep/20260911-f6440c3/c5-regression/`。历史目录 `docs/d2/d2d/20260910-c643439/` **未改写**。

---

## 5. 插件检查清单（预期应看到）

1. 有限默认输入下，合法 mask 的 −∞ 不报错；只把 `A[6][0]` 改成 NaN 时采样失败。
2. Forced(1,0)、Forced(-1,0)、Forced(1,1) 的 `RuntimePack` 哈希不同。
3. `disable_active_motion` 后 Forced/Oracle 不能再写出推进/转向费；光合仍可发生。
4. 探针开/关不改变 `state_hash`；超过 256 帧后内存帧数仍有界。
5. 学习/冻结从同一 `.eus1` 分出，只改冻结开关；同 seed 的 common 初态哈希应与 C3/C4/learn/freeze 相同（见 `runs.csv` 的 `initial_hash`）。
6. T_recover 用有符号 `P=E_old+E_new+8C`，不能 `saturating_sub`。
7. CLI 没有“不带 subcommand 就开跑正式评估”的默认入口。
8. 没有擅自改 actor_lr / 奖励系数 / 网络形状。

若数字与第 3–4 节不一致，以 CSV / `statistics.json` 为准，并视为批次问题，而不是改报告凑数。
