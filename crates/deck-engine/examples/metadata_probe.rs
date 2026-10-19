//! Read-only metadata diagnostics; never decodes PCM or writes source files.
fn main() {
    for path in std::env::args().skip(1) {
        let started = std::time::Instant::now();
        let metadata = deck_engine::read_file_metadata(std::path::Path::new(&path));
        println!(
            "{}",
            serde_json::json!({"path":path,"metadata":metadata,"ms":started.elapsed().as_secs_f64()*1000.})
        );
    }
}
