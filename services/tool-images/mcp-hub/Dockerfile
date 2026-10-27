# The Nestbox MCP hub: the sandbox's own files and shell as MCP tools, over HTTP.
#
# HTTP, not stdio, on purpose. A stdio MCP server runs inside the container and
# is invisible to the egress proxy; an HTTP server on the sandbox network is
# reached through the proxy, so every tool call appears in the traffic log with
# a tool name, its (redacted) arguments, and how long it took. That is the point
# of shipping our own server: what an agent does becomes an observable fact.
#
# Standard library only: this image has no package manager step, so rebuilding it
# cannot be affected by a registry outage or a hijacked dependency.

FROM python:3.12-slim

RUN useradd --create-home --uid 10002 mcp
WORKDIR /app
COPY server.py /app/server.py

USER mcp
EXPOSE 8080
ENV PYTHONUNBUFFERED=1
CMD ["python3", "/app/server.py"]
