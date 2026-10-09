export function step(state: { startedAt: number }, event: "start") {
  return [{ startedAt: Date.now() }, []] as const;
}
