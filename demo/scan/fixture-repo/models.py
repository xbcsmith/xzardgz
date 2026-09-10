"""Data model classes for the pipeline."""

from dataclasses import dataclass
from typing import Any


@dataclass
class Record:
    """A single data record read from the input file.

    Attributes:
        id: Unique identifier string for this record.
        value: The record payload. May be None for records that should be
            filtered out by the pipeline.
    """

    id: str
    value: Any


@dataclass
class TransformResult:
    """Summary of a completed pipeline run.

    Attributes:
        total: Total number of records read from the input file.
        processed: Number of records written to the output file after
            filtering out records with a None value.
    """

    total: int
    processed: int

    @property
    def dropped(self) -> int:
        """Number of records that were filtered out."""
        return self.total - self.processed
