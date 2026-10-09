pub struct State {
    pub balance: i64,
}

pub fn step(state: State, amount: i64) -> State {
    assert!(amount > 0, "a withdrawal is positive");
    assert!(amount <= state.balance, "cannot withdraw more than the balance");
    let next = State { balance: state.balance - amount };
    debug_assert!(next.balance >= 0, "the balance never goes negative");
    next
}
