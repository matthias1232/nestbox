FROM alpine:3.20
RUN apk add --no-cache git
ENTRYPOINT ["sleep", "infinity"]

# nestbox-sandbox-user: the sandbox does not run as root.
#
# A gateway (OpenShell) sandbox refuses to start when its working directory
# belongs to another user than the sandbox identity, and an agent that signs in
# on a root account has the whole container to damage. The user is added here,
# after every install step, so the image layers above stay cached.
RUN id -u nestbox >/dev/null 2>&1 || useradd -m -d /home/nestbox -u 10001 -s /bin/bash nestbox \
 && mkdir -p /workspace /home/nestbox \
 && chown -R nestbox /workspace /home/nestbox
USER nestbox
WORKDIR /workspace
