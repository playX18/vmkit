use criterion::{criterion_group, criterion_main, Criterion};
use swapstack::coroutine::*;
pub fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("resume", |b| {
        let f = Coroutine::new(|mut ctx| loop {
            ctx = ctx.resume();
        });
        let mut x = Some(f);
        b.iter(|| {
            x = Some(x.take().unwrap().resume());
        });
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
