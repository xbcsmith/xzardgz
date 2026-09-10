"""Database query helpers."""

import logging

logger = logging.getLogger(__name__)

# In-memory store used only in the demo fixture.
# A real service would connect to an actual database.
_USERS: dict[str, dict] = {}


def get_user_by_id(user_id: str) -> dict | None:
    """Return the user record for the given identifier, or None.

    Args:
        user_id: The user identifier to look up.

    Returns:
        The user dict, or None if not found.
    """
    result = _USERS.get(user_id)
    if result is None:
        logger.debug("user not found: %s", user_id)
    return result


def add_user(user_id: str, password_hash: str) -> None:
    """Insert a new user record into the in-memory store.

    Args:
        user_id: Unique identifier for the new user.
        password_hash: Pre-hashed password to store.
    """
    _USERS[user_id] = {"id": user_id, "password_hash": password_hash}
