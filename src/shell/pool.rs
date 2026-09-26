use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

/// Maps `f` over `items` on at most `limit` threads, keeping the order of `items`.
pub fn map<T: Sync, R: Send>(items: &[T], limit: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<R>>> = Mutex::new(items.iter().map(|_| None).collect());
    thread::scope(|scope| {
        for _ in 0..limit.clamp(1, items.len().max(1)) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(index) else { break };
                    let result = f(item);
                    results
                        .lock()
                        .expect("no worker panics while holding the lock")[index] = Some(result);
                }
            });
        }
    });
    results
        .into_inner()
        .expect("no worker panics while holding the lock")
        .into_iter()
        .map(|r| r.expect("every item was processed"))
        .collect()
}
