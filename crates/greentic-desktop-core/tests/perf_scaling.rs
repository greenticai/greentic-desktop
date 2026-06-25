use greentic_desktop_core::checksum_workload;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

fn run_workload(threads: usize) -> Duration {
    let start = Instant::now();
    let (tx, rx) = mpsc::channel();

    for _ in 0..threads {
        let tx = tx.clone();
        thread::spawn(move || {
            let checksum = checksum_workload(25_000);
            tx.send(checksum).expect("receiver should remain open");
        });
    }

    drop(tx);

    let results: Vec<_> = rx.into_iter().collect();
    assert_eq!(results.len(), threads);
    assert!(results.iter().all(|result| *result == results[0]));

    start.elapsed()
}

#[test]
fn concurrent_workload_completes_before_timeout() {
    let elapsed = run_workload(8);
    assert!(
        elapsed < Duration::from_secs(5),
        "concurrent workload exceeded timeout: {elapsed:?}"
    );
}

#[test]
fn scaling_should_not_degrade_badly() {
    let t1 = run_workload(1);
    let t4 = run_workload(4);
    let t8 = run_workload(8);

    assert!(
        t4 <= t1.mul_f64(12.0).max(Duration::from_millis(25)),
        "4 threads slower than expected: t1={t1:?}, t4={t4:?}"
    );

    assert!(
        t8 <= t4.mul_f64(12.0).max(Duration::from_millis(25)),
        "8 threads slower than expected: t4={t4:?}, t8={t8:?}"
    );
}
