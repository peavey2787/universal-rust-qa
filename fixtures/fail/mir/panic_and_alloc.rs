#[qa_attr::no_panic]
#[qa_attr::no_alloc]
pub fn violating(value: usize) -> Vec<u8> {
    assert!(value < 1024);
    vec![0; value]
}
