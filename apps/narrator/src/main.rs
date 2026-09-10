//! Periodic narrator CLI (ch. 31).

use narrator::{plan_windows, run_window};
use universe_archive::Archive;
use universe_types::{PresentationSpec, RuleSpec};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("narrator <archive-dir>");
        eprintln!("只读叙述；backend_profile=null 时使用确定性模板。");
        return;
    };
    let spec = RuleSpec::eu_core_1();
    let present = PresentationSpec::default();
    match Archive::open(&dir, &spec) {
        Ok(arch) => {
            let p = arch.progress().unwrap_or_default();
            let ranges = plan_windows(p.narrated_tick, p.durable_tick, present.window_ticks, true);
            if ranges.is_empty() {
                println!(
                    "no ready window; narrated={} durable={}",
                    p.narrated_tick, p.durable_tick
                );
                return;
            }
            for r in ranges {
                match run_window(&arch, &spec, &present, r.a, r.b) {
                    Ok(body) => println!("{body}"),
                    Err(e) => eprintln!("narrate [{}, {}) failed: {e}", r.a, r.b),
                }
            }
        }
        Err(e) => eprintln!("archive open error: {e}"),
    }
}
