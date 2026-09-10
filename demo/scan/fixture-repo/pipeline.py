#!/usr/bin/env python3
"""Data pipeline entry point.

Reads a JSON input file, runs the configured transform steps, and writes
the processed result to an output file.
"""

import argparse
import json
import sys

from models import Record, TransformResult


def run_pipeline(input_path: str, output_path: str) -> TransformResult:
    """Run the full pipeline from input file to output file.

    Args:
        input_path: Path to the JSON input file.
        output_path: Path where the JSON output will be written.

    Returns:
        A TransformResult describing what was processed.

    Raises:
        FileNotFoundError: If input_path does not exist.
        ValueError: If the input file does not contain a JSON array.
    """
    with open(input_path, encoding="utf-8") as fh:
        raw = json.load(fh)

    if not isinstance(raw, list):
        raise ValueError(f"expected a JSON array in {input_path}, got {type(raw).__name__}")

    records = [Record(id=item["id"], value=item["value"]) for item in raw]
    processed = [r for r in records if r.value is not None]

    with open(output_path, "w", encoding="utf-8") as fh:
        json.dump([{"id": r.id, "value": r.value} for r in processed], fh, indent=2)

    return TransformResult(total=len(records), processed=len(processed))


def main() -> None:
    """Parse arguments and run the pipeline."""
    parser = argparse.ArgumentParser(description="Run the data pipeline.")
    parser.add_argument("--input", required=True, help="Path to JSON input file.")
    parser.add_argument("--output", required=True, help="Path for JSON output file.")
    args = parser.parse_args()

    try:
        result = run_pipeline(args.input, args.output)
        print(f"Processed {result.processed}/{result.total} records.")
    except (FileNotFoundError, ValueError) as exc:
        print(f"Error: {exc}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
