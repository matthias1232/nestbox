# security/ — identity, redaction, headers

## Purpose

The cross-cutting controls that would be forgotten if each module implemented
them separately: password hashing, session and CSRF tokens, secret redaction,
and the response headers that make a browser treat the console safely.

This module exists as one folder so there is exactly one implementation of each
control. A second implementation is how a control gets weakened in one place and
not another.

## Dependencies

- May use: `domain`, `argon2`, `sha2`, `rand`, `base64`, `axum` (for headers and
  response types)
- Must not use: `sqlx` directly. Persistence goes through `store/`.

## Security rules

1. A password is hashed with Argon2id. The plaintext is never stored, logged, or
   returned, and never compared with `==`.
2. A session token and a CSRF token are random, and only their SHA-256 hash is
   stored. A database dump therefore yields no usable session.
3. Redaction is applied at the boundary (log, error, event), never at the source.
   A value that is secret must be masked on the way out, so forgetting one call
   site cannot leak it.
4. Comparison of a hash is constant-time. A byte-by-byte compare leaks timing
   that is enough to forge a token one byte at a time.
5. Headers are set on every response, including errors and streams. A CSP that
   is present only on success is not a CSP.

## Conventions

- Every function here is small and total: it either returns a value or a typed
  error, and it never panics on input.
- The list of sensitive key names is one constant. Adding a credential type
  means adding it there, once.

## Files

| File | Responsibility |
|---|---|
| `password.rs` | Argon2id hashing and verification |
| `session.rs` | Session and CSRF token generation, hashing, comparison |
| `redaction.rs` | Masking secrets in strings and JSON values |
| `headers.rs` | CSP, HSTS, frame options, CORS policy |

## Extending this module

If you are adding a new secret-shaped field, add its key name to
`redaction::SENSITIVE_KEYS` **first**, then write the code that handles it. That
order is deliberate: the mask exists before the value does.
