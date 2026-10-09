type State = { balance: number };

function invariant(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

export function step(state: State, amount: number): State {
  invariant(amount > 0, "a withdrawal is positive");
  invariant(amount <= state.balance, "cannot withdraw more than the balance");
  const next = { balance: state.balance - amount };
  invariant(next.balance >= 0, "the balance never goes negative");
  return next;
}
