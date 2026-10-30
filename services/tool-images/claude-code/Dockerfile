# Claude Code in a sandbox, for SSH: `ssh -t ... nestbox@node claude`.
#
# The version is pinned by an argument rather than `latest`: the tag
# `nestbox-claude-code:local` is what the control plane records, and the build log
# holds the resolved version. The credential is NOT baked in: ANTHROPIC_API_KEY
# arrives in the sandbox environment, or the operator signs in on first run.
#
# Runs as `nestbox`, the user the SSH door and the agents use, so a login lands in
# the home the agent works in.

FROM node:22-bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive
ARG CLAUDE_CODE_VERSION=2.1.312

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

# A user-level prefix: no root needed at run time, and `docker exec` and the SSH
# door find the command without a login shell reading ~/.bashrc.
ENV NPM_CONFIG_PREFIX=/home/nestbox/.local
ENV PATH=/home/nestbox/.local/bin:${PATH}

RUN npm install -g "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}" \
    && claude --version

CMD ["sleep", "infinity"]
