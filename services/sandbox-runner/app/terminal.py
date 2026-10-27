"""The PTY side of the browser terminal.

Skeleton on purpose. The transport is decided and documented here, the pump is
not written yet: opening a PTY, resizing it, and closing it without leaking a
`docker exec` socket is a piece of work that deserves its own tests, and a
half-written pump is worse than an honest refusal.

What is already fixed, because the control API and the console are being built
against it:

* `GET /v1/sandboxes/{name}/terminal` upgrades to a WebSocket;
* the client sends text frames as terminal input and one JSON control frame for
  resize — `{"type":"resize","cols":80,"rows":24}`;
* the server sends text frames as terminal output and one control frame before
  it closes — `{"type":"close","reason":"idle" | "sandbox_stopped" | "error"}`;
* authentication is the node token, in `Sec-WebSocket-Protocol` or the `token`
  query parameter. Without a token the upgrade is refused; an unset
  `NESTBOX_NODE_TOKEN` refuses everything rather than allowing everything.
"""
from __future__ import annotations

import os

from fastapi import APIRouter, WebSocket, WebSocketDisconnect

router = APIRouter()

CLOSE_NOT_IMPLEMENTED = 1011
CLOSE_UNAUTHORIZED = 1008


def authorised(websocket: WebSocket) -> bool:
    """Whether the handshake carries the node token.

    Both forms are accepted because a browser cannot set headers on a WebSocket
    and an operator's `websocat` can. The comparison is on the whole string, and
    an absent token on the server side refuses: a missing secret must never mean
    "no check".
    """
    expected = os.getenv("NESTBOX_NODE_TOKEN", "")
    if not expected:
        return False
    offered = websocket.query_params.get("token")
    if not offered:
        for protocol in websocket.headers.get("sec-websocket-protocol", "").split(","):
            candidate = protocol.strip()
            if candidate and candidate != "nestbox-terminal":
                offered = candidate
                break
    return offered == expected


@router.websocket("/v1/sandboxes/{name}/terminal")
async def terminal(websocket: WebSocket, name: str) -> None:
    """Attach a PTY to the container, once the pump is written."""
    if not authorised(websocket):
        await websocket.close(code=CLOSE_UNAUTHORIZED, reason="node token required")
        return
    await websocket.accept(subprotocol="nestbox-terminal")
    try:
        await websocket.send_json(
            {
                "type": "close",
                "reason": "error",
                "message": f"the terminal for {name} is not implemented yet",
            }
        )
    except WebSocketDisconnect:
        return
    await websocket.close(code=CLOSE_NOT_IMPLEMENTED, reason="terminal not implemented")
