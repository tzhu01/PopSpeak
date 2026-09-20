// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "--release-self-check") {
        // Deliberately run before any windows, user data or single-instance locks.
        let Some(output) = args.get(2).filter(|_| args.len() == 3) else {
            std::process::exit(2);
        };
        let output = std::path::Path::new(output);
        if !output.is_absolute() {
            std::process::exit(2);
        }
        let report = popspeak_lib::release_self_check();
        let passed = report["custom_protocol"] == true
            && report["embedded_index_html"] == true
            && report["embedded_asset_count"].as_u64().unwrap_or(0) >= 2;
        if std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).is_err() {
            std::process::exit(2);
        }
        std::process::exit(if passed { 0 } else { 1 });
    }
    popspeak_lib::run()
}
