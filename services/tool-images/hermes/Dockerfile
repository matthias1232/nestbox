FROM ubuntu:24.04
RUN apt-get update && apt-get install -y curl git python3 nodejs npm && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home nestbox
USER nestbox
WORKDIR /home/nestbox
RUN curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash
CMD ["sleep", "infinity"]
