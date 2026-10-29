# The media MCP server: render image/video/music, over HTTP.
#
# HTTP, not stdio, for the same reason as the Nestbox hub: an HTTP MCP server is
# reached through the sandbox's egress proxy, so every `tools/call` appears in the
# traffic log with a tool name, its redacted arguments and a duration. A stdio
# server would be invisible by construction.
#
# Standard library only. This image has no package manager step, so a rebuild
# cannot be affected by a registry outage or a hijacked dependency.

FROM python:3.12-slim

RUN useradd --create-home --uid 10003 media
WORKDIR /app
COPY server.py /app/server.py

USER media
EXPOSE 8080
ENV PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1

# Liveness without a shell client: the slim image has no curl, and adding one to
# answer "are you alive" would be a package for a one-line request.
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["python3", "-c", "import sys, urllib.request; sys.exit(0 if urllib.request.urlopen('http://127.0.0.1:8080/healthz', timeout=2).status == 200 else 1)"]

CMD ["python3", "/app/server.py"]
