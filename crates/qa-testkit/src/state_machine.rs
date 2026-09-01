pub trait StateMachine<Event> {
    type Error: std::fmt::Debug;
    fn apply(&mut self, event: &Event) -> Result<(), Self::Error>;
    fn invariant(&self) -> bool;
}
pub fn assert_trace<M, E>(machine: &mut M, events: &[E])
where
    M: StateMachine<E>,
{
    assert!(machine.invariant(), "initial state invariant failed");
    for event in events {
        machine.apply(event).unwrap_or_else(|error| panic!("state transition failed: {error:?}"));
        assert!(machine.invariant(), "state invariant failed after transition");
    }
}
pub fn assert_rejected<M, E>(machine: &mut M, event: &E)
where
    M: StateMachine<E>,
{
    assert!(machine.apply(event).is_err(), "illegal transition was accepted");
    assert!(machine.invariant(), "state invariant failed after rejected transition");
}
