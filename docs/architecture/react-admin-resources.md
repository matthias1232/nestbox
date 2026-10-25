# React Admin Resource Contract
The planned local Control Center uses React Admin and the FastAPI control-api under /v1 only.
Resources: sandboxes, policies, mounts, tool-catalog, events and artifacts.
Every mutation must surface audit status; the browser never receives host privileges or ordinary sandbox secrets.
