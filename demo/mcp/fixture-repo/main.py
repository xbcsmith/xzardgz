#!/usr/bin/env python3
"""Greeting service entry point."""

from utils import greet


def main() -> None:
    """Run the greeting service and print a greeting to standard output."""
    print(greet("world"))


if __name__ == "__main__":
    main()
