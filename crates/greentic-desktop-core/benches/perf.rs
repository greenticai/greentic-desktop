use criterion::{criterion_group, criterion_main, Criterion};
use greentic_desktop_core::{checksum_workload, normalize_capabilities, Capability, RiskLevel};

fn bench_checksum_workload(c: &mut Criterion) {
    c.bench_function("checksum_workload_10k", |b| {
        b.iter(|| checksum_workload(10_000))
    });
}

fn bench_capability_normalization(c: &mut Criterion) {
    let capabilities = vec![
        Capability {
            name: "replay".to_owned(),
            adapter: "web".to_owned(),
            risk: RiskLevel::Medium,
        },
        Capability {
            name: "info".to_owned(),
            adapter: "core".to_owned(),
            risk: RiskLevel::Low,
        },
        Capability {
            name: "mcp-serve".to_owned(),
            adapter: "runtime".to_owned(),
            risk: RiskLevel::High,
        },
    ];

    c.bench_function("normalize_capabilities", |b| {
        b.iter(|| normalize_capabilities(capabilities.clone()).expect("valid capabilities"))
    });
}

criterion_group!(
    benches,
    bench_checksum_workload,
    bench_capability_normalization
);
criterion_main!(benches);
