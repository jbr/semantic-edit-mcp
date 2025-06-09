mod snapshot_runner;

use snapshot_runner::SnapshotRunner;
use std::env;

#[tokio::test]
async fn run_snapshot_tests() {
    // Check for --update flag in environment, test args, or flag file
    let update_mode = env::var("UPDATE_SNAPSHOTS").is_ok() 
        || env::args().any(|arg| arg == "--update")
        || std::path::Path::new("UPDATE_SNAPSHOTS").exists();

    let runner = SnapshotRunner::new(update_mode)
        .expect("Failed to create snapshot runner");

    let results = runner.run_all_tests().await
        .expect("Failed to run snapshot tests");

    runner.print_summary(&results);

    // In verify mode, fail if any tests failed
    if !update_mode {
        let failed_count = results.iter().filter(|r| !r.passed).count();
        if failed_count > 0 {
            panic!("❌ {} snapshot test(s) failed", failed_count);
        }
    }
}
