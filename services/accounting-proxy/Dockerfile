# The Nestbox accounting proxy: agent model traffic, accounted and inspected.
#
# The agent's provider endpoint points here instead of at the provider. That is
# the whole design: one place sees the request before it leaves, one place turns
# the answer into a metadata row, and no provider credential is ever stored —
# the caller's own header is forwarded untouched.
#
# Standard library only, for the same reason as the MCP hub: this image has no
# package manager step, so rebuilding it cannot be affected by a registry outage
# or a hijacked dependency.

FROM python:3.12-slim

RUN useradd --create-home --uid 10003 accounting
WORKDIR /app
COPY server.py /app/server.py

USER accounting
EXPOSE 8099
ENV PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    NESTBOX_ACCOUNTING_PROXY_PORT=8099

# Liveness without a shell client: the slim image has no curl, and adding one to
# answer "are you alive" would be a package for a one-line request.
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["python3", "-c", "import os, sys, urllib.request; port = os.environ.get('NESTBOX_ACCOUNTING_PROXY_PORT', '8099'); sys.exit(0 if urllib.request.urlopen(f'http://127.0.0.1:{port}/healthz', timeout=2).status == 200 else 1)"]

CMD ["python3", "/app/server.py"]
