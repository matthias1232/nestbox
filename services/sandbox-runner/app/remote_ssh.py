"""Run a script over SSH on a machine that is not a node yet (ADR 0057).

The credentials arrive with the request, are used for this call only, and are
never written anywhere. The endpoint exists so that "enter the public IP of a
new machine" works without opening a port or copying a script by hand;
package B19 fills the implementation.
"""

from fastapi import APIRouter, HTTPException

router = APIRouter()


@router.post("/v1/remote/ssh-run")
async def ssh_run() -> dict:
    """Run one script over SSH (not built yet)."""
    raise HTTPException(status_code=501, detail="not built yet")
