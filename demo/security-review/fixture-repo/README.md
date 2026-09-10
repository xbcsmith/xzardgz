# Auth Service

A minimal Python authentication service used as the demo scan target for the
XZardgz security review plugin. The code contains patterns a security review
will analyse: credential handling, input validation, and error logging.

## Structure

- `app.py` - HTTP handler entry point.
- `auth.py` - Token verification and session helpers.
- `db.py` - Database query helpers.
- `requirements.txt` - Runtime dependencies.

## Running

```bash
python app.py --port 8080
```
