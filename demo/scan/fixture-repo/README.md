# Data Pipeline

A minimal Python data processing library used as the demo scan target for
XZardgz. It demonstrates what the scanner discovers when it walks a repository:
file counts by language, dependency manifests, and directory structure.

## Structure

- `pipeline.py` - Main pipeline runner. Reads input, runs transforms, writes
  output.
- `models.py` - Typed data model classes used by the pipeline.
- `requirements.txt` - Runtime dependencies.

## Running

```bash
python pipeline.py --input data.json --output result.json
```
