# OpenChamber (the web front end) and OpenCode (the agent it drives).
#
# Both are pinned by argument, not `latest`. The web UI listens on 3000 and the
# control plane publishes that port on the node; `entrypoint.sh` makes the UI
# password-protected before it does.

FROM node:22-bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive
ARG OPENCHAMBER_VERSION=2.5.3
ARG OPENCODE_VERSION=1.19.5

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates curl git jq less procps htop ripgrep vim tmux openssh-client python3 \
    && rm -rf /var/lib/apt/lists/*

# Graphify: the local knowledge-graph MCP server an agent can be given.
#
# Installed here rather than in the home at run time, so a sandbox starts with the
# command already on PATH and an offline sandbox still has it. A venv keeps it off
# the system Python (PEP 668), and the pin is deliberate: the tool reads a graph
# format, and a silent upgrade would change what the agent sees.
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3-venv \
    && rm -rf /var/lib/apt/lists/* \
    && python3 -m venv /opt/graphify \
    && /opt/graphify/bin/pip install --no-cache-dir graphifyy==0.7.5 \
    && ln -sf /opt/graphify/bin/graphify /usr/local/bin/graphify \
    && graphify --help >/dev/null

RUN useradd --create-home --shell /bin/bash nestbox
USER nestbox
WORKDIR /home/nestbox

ENV NPM_CONFIG_PREFIX=/home/nestbox/.local
ENV PATH=/home/nestbox/.local/bin:${PATH}

RUN npm install -g "@openchamber/web@${OPENCHAMBER_VERSION}" "opencode-ai@${OPENCODE_VERSION}" \
    && openchamber --version \
    && opencode --version

COPY --chown=nestbox:nestbox entrypoint.sh /home/nestbox/.local/bin/nestbox-openchamber

EXPOSE 3000
CMD ["/home/nestbox/.local/bin/nestbox-openchamber"]
