//! F2 preparation CLI: generate / validate / dev / recompute. No formal-eval default.

use std::path::PathBuf;
use std::time::Instant;

use universe_lab::f2::{
    default_out_dir, generate_into, recompute_file, replay_checks_json, run_dev, validate_dir,
    write_prep_docs, InitRecipe,
};

fn usage() -> ! {
    eprintln!(
        "usage:\n  cargo run --release --offline -p universe_lab --bin f2 -- generate [dir]\n  cargo run --release --offline -p universe_lab --bin f2 -- validate [dir]\n  cargo run --release --offline -p universe_lab --bin f2 -- dev [dir]\n  cargo run --release --offline -p universe_lab --bin f2 -- recompute [dir]\n\nNo default formal-evaluation entry. Development seeds are 10001–10012."
    );
    std::process::exit(2);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let cmd = match args.next() {
        Some(c) => c,
        None => usage(),
    };
    let dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(default_out_dir);
    match cmd.as_str() {
        "generate" => match generate_into(&dir, &InitRecipe::v1_left_body_keep_residual_q()) {
            Ok(h) => {
                eprintln!("generated {} protocol={}", dir.display(), universe_types::hex_hash(&h));
            }
            Err(e) => {
                eprintln!("generate failed: {e}");
                std::process::exit(1);
            }
        },
        "validate" => match validate_dir(&dir) {
            Ok(()) => eprintln!("validate ok {}", dir.display()),
            Err(e) => {
                eprintln!("validate failed: {e}");
                std::process::exit(1);
            }
        },
        "dev" => {
            let t0 = Instant::now();
            match run_dev(&dir) {
                Ok(s) => {
                    let replay = replay_checks_json(&dir).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
                    let _ = std::fs::write(dir.join("replay_checks.json"), replay);
                    let elapsed = t0.elapsed().as_millis();
                    if let Err(e) = write_prep_docs(&dir, &s, elapsed) {
                        eprintln!("docs failed: {e}");
                    }
                    eprintln!(
                        "dev recipe={} blocked={} c3={}/12 c4={}/12 neg_fp={} residual_rec={} learn={} freeze={} mean={:?} elapsed={:?}",
                        s.recipe,
                        s.blocked,
                        s.c3_recovered,
                        s.c4_recovered,
                        s.neg_false_positive,
                        s.residual_recovered,
                        s.learn_recovered,
                        s.freeze_recovered,
                        s.mean,
                        t0.elapsed()
                    );
                    if s.blocked || s.engineering > 0 {
                        std::process::exit(2);
                    }
                }
                Err(e) => {
                    eprintln!("dev failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        "recompute" => {
            let ticks = dir.join("tick_metrics");
            let rd = match std::fs::read_dir(&ticks) {
                Ok(x) => x,
                Err(e) => {
                    eprintln!("recompute: {e}");
                    std::process::exit(1);
                }
            };
            let mut n = 0u32;
            let mut mismatch = 0u32;
            for ent in rd.flatten() {
                let p = ent.path();
                if p.extension().and_then(|s| s.to_str()) != Some("csv") {
                    continue;
                }
                match recompute_file(&p) {
                    Ok(_) => n += 1,
                    Err(e) => {
                        eprintln!("{}: {e}", p.display());
                        mismatch += 1;
                    }
                }
            }
            eprintln!("recompute files={n} errors={mismatch}");
            if mismatch > 0 {
                std::process::exit(2);
            }
        }
        _ => usage(),
    }
}
