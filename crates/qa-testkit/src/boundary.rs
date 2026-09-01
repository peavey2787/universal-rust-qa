pub fn u64_boundaries() -> [u64; 8] {
    [0, 1, 2, u64::MAX, u64::MAX - 1, u64::MAX / 2, 255, 256]
}
pub fn i64_boundaries() -> [i64; 7] {
    [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX]
}
