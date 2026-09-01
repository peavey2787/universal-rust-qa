pub fn assert_roundtrip<T: PartialEq + std::fmt::Debug, E: std::fmt::Debug>(
    value: T,
    encode: impl FnOnce(&T) -> Vec<u8>,
    decode: impl FnOnce(&[u8]) -> Result<T, E>,
) {
    let bytes = encode(&value);
    match decode(&bytes) {
        Ok(decoded) => assert_eq!(decoded, value),
        Err(error) => panic!("round-trip decode failed: {error:?}"),
    }
}
