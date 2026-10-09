type State = { balance: number };

export const step = (state: State, amount: number): State => ({
  balance: state.balance - amount,
});
