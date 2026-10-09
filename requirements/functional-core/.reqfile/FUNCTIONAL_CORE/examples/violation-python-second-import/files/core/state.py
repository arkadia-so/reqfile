import json, os


def load(path):
    return json.loads(os.path.join(path))
