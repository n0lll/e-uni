//! Write D2-D C5 artefacts under docs/d2/d2d/<run_id> (or a given directory).

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "docs/d2/d2d/latest".to_string());
    match universe_lab::d2d::run_d2d_campaign(&out) {
        Ok(report) => {
            eprintln!(
                "D2-D wrote {out} valid_complete={} rows={}",
                report.integrity.valid_complete, report.integrity.n_rows
            );
            if !report.integrity.valid_complete {
                std::process::exit(2);
            }
        }
        Err(e) => {
            eprintln!("D2-D failed: {e}");
            std::process::exit(1);
        }
    }
}
