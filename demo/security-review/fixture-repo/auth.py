"""Token verification and session management helpers."""

import hashlib
import os
import time


def verify_token(plaintext: str, stored_hash: str) -> bool:
    """Verify a plaintext password against a stored hash.

    Uses SHA-256 for demonstration. Production code should use bcrypt or
    argon2 with a per-record salt.

    Args:
        plaintext: The plaintext password to verify.
        stored_hash: The stored password hash to compare against.

    Returns:
        True if the hashes match, False otherwise.
    """
    candidate_hash = hashlib.sha256(plaintext.encode()).hexdigest()
    return candidate_hash == stored_hash


def create_session(user_id: str) -> dict:
    """Create a new session for the given user.

    Args:
        user_id: The user identifier to associate with the session.

    Returns:
        A dict containing ``token``, ``user_id``, and ``expires_at``.
    """
    token = os.urandom(32).hex()
    return {
        "token": token,
        "user_id": user_id,
        "expires_at": int(time.time()) + 3600,
    }
