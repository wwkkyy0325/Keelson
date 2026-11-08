# Polyglot workspace

A small fixture for the check selector. `frontend/` is a Node package with a
passing test script; `backend/` is a Python project whose test intentionally
fails. A turn that changes only `frontend/` must run the Node check and skip
the Python one; a turn that changes `backend/` must do the opposite.
