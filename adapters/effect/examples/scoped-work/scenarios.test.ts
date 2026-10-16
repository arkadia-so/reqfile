import { Effect, Exit } from "effect";
import { expect, test } from "vitest";

async function closedScope() {
  const state = { releases: 0, aborted: false, value: undefined as string | undefined };
  let complete: (value: string) => void = () => {};
  let started: () => void = () => {};
  const ready = new Promise<void>((resolve) => { started = resolve; });
  const task = Effect.scoped(Effect.gen(function* () {
    yield* Effect.acquireRelease(Effect.succeed(undefined), () => Effect.sync(() => { state.releases += 1; }));
    const value = yield* Effect.tryPromise({
      try: (signal) => new Promise<string>((resolve) => {
        complete = resolve;
        signal.addEventListener("abort", () => { state.aborted = true; });
        started();
      }),
      catch: (cause) => cause,
    });
    state.value = value;
  }));
  await Effect.runPromise(Effect.scoped(Effect.gen(function* () {
    yield* task.pipe(Effect.forkScoped);
    yield* Effect.tryPromise({ try: () => ready, catch: (cause) => cause });
  })));
  return { state, complete };
}

test("owner close interrupts active work", async () => {
  const { state } = await closedScope();
  expect(state.aborted).toBe(true);
  expect(state.releases).toBe(1);
});

test("late completion cannot commit", async () => {
  const { state, complete } = await closedScope();
  complete("obsolete response");
  await Promise.resolve();
  expect(state.value).toBeUndefined();
});

test("failure releases the acquired resource once", async () => {
  let releases = 0;
  const result = await Effect.runPromiseExit(Effect.scoped(Effect.gen(function* () {
    yield* Effect.acquireRelease(Effect.succeed(undefined), () => Effect.sync(() => { releases += 1; }));
    return yield* Effect.fail("dependency unavailable");
  })));
  expect(Exit.isFailure(result)).toBe(true);
  expect(releases).toBe(1);
});

test("Effect scope interruption reaches the promise boundary", async () => {
  const { state, complete } = await closedScope();
  expect(state.aborted).toBe(true);
  complete("finished after interruption");
  await Promise.resolve();
  expect(state.value).toBeUndefined();
  expect(state.releases).toBe(1);
});
