#!/usr/bin/env python3
"""Auth service HTTP handler entry point."""

import argparse
import json
import logging
import sys

from db import get_user_by_id

from auth import create_session, verify_token

logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
logger = logging.getLogger(__name__)


def handle_login(username: str, password: str) -> dict:
    """Handle a login request.

    Args:
        username: The submitted username.
        password: The submitted password in plaintext.

    Returns:
        A response dict with ``token`` on success or ``error`` on failure.
    """
    user = get_user_by_id(username)
    if user is None:
        logger.warning("login attempt for unknown user: %s", username)
        return {"error": "invalid credentials"}

    if not verify_token(password, user["password_hash"]):
        logger.warning("failed login for user: %s", username)
        return {"error": "invalid credentials"}

    session = create_session(user["id"])
    logger.info("login success for user: %s", username)
    return {"token": session["token"]}


def handle_request(raw_body: str) -> str:
    """Parse and dispatch an incoming request.

    Args:
        raw_body: Raw JSON body string from the HTTP request.

    Returns:
        JSON-encoded response string.
    """
    try:
        body = json.loads(raw_body)
    except json.JSONDecodeError:
        return json.dumps({"error": "invalid JSON"})

    action = body.get("action", "")
    if action == "login":
        result = handle_login(body.get("username", ""), body.get("password", ""))
    else:
        result = {"error": f"unknown action: {action}"}

    return json.dumps(result)


def main() -> None:
    """Parse CLI arguments and start the service."""
    parser = argparse.ArgumentParser(description="Auth service.")
    parser.add_argument("--port", type=int, default=8080)
    args = parser.parse_args()
    logger.info("Auth service listening on port %d", args.port)


if __name__ == "__main__":
    main()
