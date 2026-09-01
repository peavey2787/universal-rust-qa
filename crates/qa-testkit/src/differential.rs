pub fn assert_differential<I: Clone, O, E: std::fmt::Debug>(
    input: I,
    reference: impl FnOnce(I) -> Result<O, E>,
    candidate: impl FnOnce(I) -> Result<O, E>,
    equivalent: impl FnOnce(&O, &O) -> bool,
) {
    let a = reference(input.clone());
    let b = candidate(input);
    match (a, b) {
        (Ok(a), Ok(b)) => assert!(equivalent(&a, &b), "differential outputs diverged"),
        (Err(_), Err(_)) => {}
        (a, b) => panic!("differential outcome diverged: ref={:?} cand={:?}", a.err(), b.err()),
    }
}
