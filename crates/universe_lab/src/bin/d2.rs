//! Write D2 artefacts under docs/d2 (or a given directory).

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "docs/d2".to_string());
    match universe_lab::d2::run_d2_campaign(&out) {
        Ok(()) => eprintln!("D2 wrote {out}"),
        Err(e) => {
            eprintln!("D2 failed: {e}");
            std::process::exit(1);
        }
    }
}
