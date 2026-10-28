# bin/ — additional binaries built from this crate

| Binary | What it is |
|---|---|
| `nestbox-control-api` (`src/main.rs`) | The API process |
| `nestbox` (`src/bin/nestbox/`) | The operator CLI |

A second binary lives here rather than in its own crate because it shares the
domain vocabulary and the error vocabulary of the API. It still talks HTTP,
never to the database: a CLI that could bypass the API would be a second way to
change state, and then two places would have to agree on permissions.
