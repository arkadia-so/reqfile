from pydantic import BaseModel


class User(BaseModel):
    name: str


class NotFound(Exception):
    pass
