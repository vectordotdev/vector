"""
Mock secret manager implementation used for testing the secrets.exec backend type.
This program is meant to be exec'ed by a unit test so that the implementation can be tested.
"""

import sys
import json
from typing import Any


class Request:
    def __init__(
        self,
        version: str,
        secrets: list[str],
        type: str | None = None,
        config: dict[str, Any] | None = None,
    ):
        self.version = version
        self.secrets = secrets
        self.type = type
        self.config = config


class Response:
    def __init__(self, contents: dict[str, dict[str, str | None]]):
        self.contents = contents


def parse_request(req: dict[str, Any]) -> Request:
    """
    Validate the request by ensuring the correct keys exist per version, and that the types of the
    respective keys are as expected as well
    """
    v1_args = set(["version", "secrets"])
    v1_1_args = set(["version", "secrets", "type", "config"])
    if "version" not in req.keys():
        raise RuntimeError("version key missing from request")
    version = req["version"]
    if version == "1.0":
        if v1_args != set(req.keys()):
            raise RuntimeError(f"Invalid required keys in 1.0 request: {req.keys()}")
        if not isinstance(req["secrets"], list):
            raise RuntimeError("key 'secrets' should be a list")
        return Request(version, req["secrets"])
    elif version == "1.1":
        if v1_1_args != set(req.keys()):
            raise RuntimeError(f"Invalid required keys in 1.1 request: {req.keys()}")
        if not isinstance(req["secrets"], list):
            raise RuntimeError("key 'secrets' should be a list")
        if not isinstance(req["type"], str):
            raise RuntimeError("key 'type' should be a str")
        if not isinstance(req["config"], dict):
            raise RuntimeError("key 'config' should be a dict")
        return Request(version, req["secrets"], req["type"], req["config"])
    else:
        raise RuntimeError(f"Invalid version detected: {version}")


def handle_request(req: Request) -> Response:
    """
    Handle the request by looking up the requested secret with a value that is in a static fake
    secret cache below. Any values not contained will return the appropriate error message.
    """
    static_fake_secrets_cache = {
        "fake_secret_1": "123456",
        "fake_secret_2": "123457",
        "fake_secret_3": "123458",
        "fake_secret_4": "123459",
        "fake_secret_5": "123460",
    }
    supported_fake_backends = ["file.json"]
    if req.version == "1.1":
        if req.type not in supported_fake_backends:
            raise RuntimeError(f"Requested backend: {req.type} not supported")
        if req.config is not None and "file_path" not in req.config:
            raise RuntimeError("File backend option file_path must be supplied")

    def get_secret(fake_secret_name: str) -> dict[str, str | None]:
        if fake_secret_name in static_fake_secrets_cache:
            return {"value": static_fake_secrets_cache[fake_secret_name], "error": None}
        else:
            return {"value": None, "error": "backend does not provide secret key"}

    return Response(dict([(s, get_secret(s)) for s in req.secrets]))


def main():
    data = sys.stdin.buffer.read()
    req = json.loads(data)
    req = parse_request(req)
    resp = handle_request(req)
    sys.stdout.write(json.dumps(resp.contents))


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        sys.stderr.write(str(e))
        sys.exit(1)
