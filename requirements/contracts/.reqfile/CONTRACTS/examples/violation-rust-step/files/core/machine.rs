pub struct State {
    pub balance: i64,
}

pub fn step(state: State, amount: i64) -> State {
    State { balance: state.balance - amount }
}
