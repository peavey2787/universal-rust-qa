use std::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::{AtomicUsize, Ordering},
};
#[derive(Debug, Clone, Copy)]
pub struct AllocationSnapshot {
    pub allocations: usize,
    pub deallocations: usize,
    pub live_bytes: usize,
    pub peak_live_bytes: usize,
}
pub struct CountingAllocator<A> {
    inner: A,
    a: AtomicUsize,
    d: AtomicUsize,
    live: AtomicUsize,
    peak: AtomicUsize,
}
impl<A> CountingAllocator<A> {
    pub const fn new(inner: A) -> Self {
        Self {
            inner,
            a: AtomicUsize::new(0),
            d: AtomicUsize::new(0),
            live: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        }
    }
    pub fn snapshot(&self) -> AllocationSnapshot {
        AllocationSnapshot {
            allocations: self.a.load(Ordering::Relaxed),
            deallocations: self.d.load(Ordering::Relaxed),
            live_bytes: self.live.load(Ordering::Relaxed),
            peak_live_bytes: self.peak.load(Ordering::Relaxed),
        }
    }
}
unsafe impl<A: GlobalAlloc> GlobalAlloc for CountingAllocator<A> {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { self.inner.alloc(l) };
        if !p.is_null() {
            self.a.fetch_add(1, Ordering::Relaxed);
            let live = self.live.fetch_add(l.size(), Ordering::Relaxed) + l.size();
            self.peak.fetch_max(live, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { self.inner.dealloc(p, l) };
        self.d.fetch_add(1, Ordering::Relaxed);
        self.live.fetch_sub(l.size(), Ordering::Relaxed);
    }
}
