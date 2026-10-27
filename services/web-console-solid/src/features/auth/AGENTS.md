# auth/ — login

## Purpose

The only screen reachable without a session. It collects an email and a
password, and nothing else.

## Rules

- The password field is never mirrored into state; it is read at submit.
- A failure shows the server's message, which is deliberately uniform so it does
  not reveal whether the account exists.
- On success the CSRF token is held in memory by the session provider only.
