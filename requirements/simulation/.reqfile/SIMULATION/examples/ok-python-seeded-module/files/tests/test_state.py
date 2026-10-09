import random

from core import state


def test_simulation_random_events():
    random.seed(42)
    current = 0
    for _ in range(1000):
        current, _ = state.step(current, random.randint(-3, 3))
        assert current >= 0
