from typing import Any


def parse(payload: Any) -> str:
    return payload["name"]
