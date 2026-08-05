use criterion::{Criterion, criterion_group, criterion_main};
use hostctl::{DataFormat, parse_entries};

fn parse_large_hosts(c: &mut Criterion) {
    let content = (0..100_000)
        .map(|index| {
            format!(
                "10.{}.{}.{} host-{index}.local # generated\n",
                (index / 65_536) % 256,
                (index / 256) % 256,
                index % 256
            )
        })
        .collect::<String>();
    c.bench_function("parse 100k hosts entries", |b| {
        b.iter(|| parse_entries(&content, DataFormat::Hosts).unwrap())
    });
}

criterion_group!(benches, parse_large_hosts);
criterion_main!(benches);
