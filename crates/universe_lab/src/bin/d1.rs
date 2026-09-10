//! Write D1 diagnostic artefacts under docs/d1 (or a given directory).

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "docs/d1".to_string());
    match universe_lab::d1::run_d1_campaign(&out) {
        Ok(()) => {
            eprintln!("D1 wrote {}", out);
        }
        Err(e) => {
            eprintln!("D1 failed: {e}");
            std::process::exit(1);
        }
    }
}
