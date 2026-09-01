use qa_testkit::*;

#[derive(Debug)]
struct Machine {
    value: i32,
}
impl StateMachine<i32> for Machine {
    type Error = &'static str;
    fn apply(&mut self, event: &i32) -> Result<(), Self::Error> {
        if *event < 0 {
            Err("negative")
        } else {
            self.value += *event;
            Ok(())
        }
    }
    fn invariant(&self) -> bool {
        (0..=10).contains(&self.value)
    }
}

#[derive(Debug)]
struct RejectingMachine {
    attempts: usize,
}
impl StateMachine<i32> for RejectingMachine {
    type Error = &'static str;
    fn apply(&mut self, _event: &i32) -> Result<(), Self::Error> {
        self.attempts += 1;
        Err("rejected")
    }
    fn invariant(&self) -> bool {
        self.attempts <= 1
    }
}

#[test]
fn boundary_sets_include_extrema_and_transition_edges() {
    assert_eq!(u64_boundaries(), [0, 1, 2, u64::MAX, u64::MAX - 1, u64::MAX / 2, 255, 256]);
    assert_eq!(i64_boundaries(), [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX]);
}

#[test]
fn deterministic_rng_repeats_for_same_seed_and_changes_state() {
    let mut a = DeterministicRng::new(42);
    let expected = [
        45_454_805_674,
        11_532_217_803_599_905_471,
        10_021_416_941_527_320_954,
        2_899_061_411_254_629_736,
    ];
    assert_eq!([a.next_u64(), a.next_u64(), a.next_u64(), a.next_u64()], expected);

    let mut b = DeterministicRng::new(42);
    assert_eq!(b.next_u64(), expected[0]);
    assert_eq!(b.next_u64(), expected[1]);
}

#[test]
fn differential_helper_accepts_equal_successes_and_equal_failures() {
    assert!(
        std::panic::catch_unwind(|| {
            assert_differential(4, |x| Ok::<_, ()>(x * 2), |x| Ok::<_, ()>(x + x), |a, b| a == b);
            assert_differential(4, |_| Err::<i32, _>("a"), |_| Err::<i32, _>("b"), |_, _| true);
        })
        .is_ok()
    );
}

#[test]
fn differential_helper_panics_on_output_or_result_shape_divergence() {
    assert!(
        std::panic::catch_unwind(|| {
            assert_differential(1, Ok::<_, ()>, |x| Ok::<_, ()>(x + 1), |a, b| a == b)
        })
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(|| {
            assert_differential(1, Ok::<_, &str>, |_| Err::<i32, _>("bad"), |a, b| a == b)
        })
        .is_err()
    );
}

#[test]
fn roundtrip_helper_accepts_valid_codec_and_panics_on_decode_error() {
    assert_roundtrip(
        7u32,
        |value| value.to_le_bytes().to_vec(),
        |bytes| Ok::<_, ()>(u32::from_le_bytes(bytes.try_into().unwrap())),
    );
    assert!(
        std::panic::catch_unwind(|| {
            assert_roundtrip(7u32, |_| vec![1], |_| Err::<u32, _>("decode"))
        })
        .is_err()
    );
}

#[test]
fn state_machine_helpers_cover_valid_trace_and_rejected_transition() {
    let mut machine = Machine { value: 0 };
    assert_trace(&mut machine, &[1, 2, 3]);
    assert_eq!(machine.value, 6);
    assert_rejected(&mut machine, &-1);
    assert_eq!(machine.value, 6);

    let mut rejecting = RejectingMachine { attempts: 0 };
    assert_rejected(&mut rejecting, &1);
    assert_eq!(rejecting.attempts, 1);
}

#[test]
fn state_machine_helper_panics_when_invariant_or_transition_fails() {
    assert!(
        std::panic::catch_unwind(|| {
            let mut machine = Machine { value: 11 };
            assert_trace(&mut machine, &[0]);
        })
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(|| {
            let mut machine = Machine { value: 0 };
            assert_trace(&mut machine, &[-1]);
        })
        .is_err()
    );
}
