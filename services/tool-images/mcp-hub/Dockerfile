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
ENV PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1

# Liveness without a shell client: the slim image has no curl, and adding one to
# answer "are you alive" would be a package for a one-line request.
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["python3", "-c", "import sys, urllib.request; sys.exit(0 if urllib.request.urlopen('http://127.0.0.1:8080/healthz', timeout=2).status == 200 else 1)"]

CMD ["python3", "/app/server.py"]
