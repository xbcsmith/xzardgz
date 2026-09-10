# Greeting Service

A minimal Python service used as the demo analysis target for XZardgz MCP
tooling. It demonstrates what XZardgz sees when it scans a repository via the
MCP filesystem server.

## Structure

- `main.py` - Entry point. Prints a greeting to standard output.
- `utils.py` - Utility module containing the `greet` function.
- `requirements.txt` - Runtime dependency list (empty for this demo).

## Running

```bash
python main.py
```

Expected output:

```text
Hello, world!
```
