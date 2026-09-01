use qa_alloc_runtime::CountingAllocator;
use std::alloc::{GlobalAlloc, Layout, System};

#[test]
fn allocator_counts_live_and_peak_bytes_across_alloc_dealloc() {
    let allocator = CountingAllocator::new(System);
    let before = allocator.snapshot();
    assert_eq!(before.allocations, 0);
    assert_eq!(before.deallocations, 0);
    assert_eq!(before.live_bytes, 0);

    let layout = Layout::from_size_align(64, 8).unwrap();
    let ptr = unsafe { allocator.alloc(layout) };
    assert!(!ptr.is_null());
    let during = allocator.snapshot();
    assert_eq!(during.allocations, 1);
    assert_eq!(during.live_bytes, 64);
    assert!(during.peak_live_bytes >= 64);

    unsafe { allocator.dealloc(ptr, layout) };
    let after = allocator.snapshot();
    assert_eq!(after.deallocations, 1);
    assert_eq!(after.live_bytes, 0);
    assert!(after.peak_live_bytes >= 64);
}
