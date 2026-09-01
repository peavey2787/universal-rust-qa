#[qa_attr::no_panic]
#[qa_attr::no_alloc]
pub fn checked_increment(value: u64) -> Option<u64> {
    value.checked_add(1)
}
