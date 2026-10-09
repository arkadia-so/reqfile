def step(state: int, event: int) -> tuple[int, list]:
    return max(0, state + event), []
