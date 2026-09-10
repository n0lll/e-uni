//! Read-only observer. Must not write authoritative world state.

use std::thread;
use std::time::Duration;

use universe_archive::Archive;
use universe_types::{hex_hash, state_hash, RuleSpec};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("observer <archive-dir> [--once]");
        eprintln!("只读快照；显示帧最多 10/真实秒，可丢帧。核心模拟不得由本进程推进。");
        return;
    };
    let once = args.any(|a| a == "--once");
    let spec = RuleSpec::eu_core_1();
    let interval = Duration::from_millis(100);
    loop {
        match Archive::open(&dir, &spec) {
            Ok(arch) => {
                let p = arch.progress().unwrap_or_default();
                if let Ok(Some((tick, path, h))) = arch.latest_snapshot_tick(p.durable_tick.max(p.live_tick))
                {
                    match Archive::load_snapshot_file(&path, Some(h)) {
                        Ok(st) => {
                            let sh = state_hash(&st)
                                .map(|x| hex_hash(&x))
                                .unwrap_or_else(|_| "err".into());
                            println!(
                                "tick={} living={} state_hash={} durable={} narrated={}",
                                tick,
                                st.organisms.len(),
                                sh,
                                p.durable_tick,
                                p.narrated_tick
                            );
                        }
                        Err(e) => eprintln!("snapshot read error: {e}"),
                    }
                } else {
                    println!(
                        "no snapshot; live={} durable={} narrated={}",
                        p.live_tick, p.durable_tick, p.narrated_tick
                    );
                }
            }
            Err(e) => eprintln!("archive open error: {e}"),
        }
        if once {
            break;
        }
        thread::sleep(interval);
    }
}
