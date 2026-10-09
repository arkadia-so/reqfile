import json

from pydantic import BaseModel


class User(BaseModel):
    age: int


def load(raw: str) -> User:
    return User.model_validate(json.loads(raw)["user"])
