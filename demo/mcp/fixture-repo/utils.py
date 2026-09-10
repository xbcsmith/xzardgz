"""Utility functions for the greeting service."""


def greet(name: str) -> str:
    """Return a personalised greeting for the given name.

    Args:
        name: The name to greet. Must not be empty.

    Returns:
        A greeting string of the form ``Hello, <name>!``.

    Raises:
        ValueError: If ``name`` is empty.
    """
    if not name:
        raise ValueError("name must not be empty")
    return f"Hello, {name}!"
