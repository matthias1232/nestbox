# Kubernetes foundation

This directory is intentionally a small deployment contract, not a duplicate of
the Compose runtime.  The OpenShell gateway has an official Helm chart; install
that chart separately and configure the Nestbox controller with its in-cluster
endpoint.

1. Publish the controller image with the GitHub workflow.
2. Copy `controller.env.example` into your secret manager and create a Secret.
3. Install the official OpenShell Helm chart in the same namespace or expose a
   gateway service reachable from the cluster.
4. Replace `CONTROLLER_IMAGE` in `controller.yaml` with your immutable image
   digest and apply `kubectl apply -k deploy/kubernetes`.

NemoClaw onboarding is a deliberate post-deployment operation.  Run it from a
controlled runner that can reach the OpenShell gateway and inject provider
credentials through your secret manager; do not place those credentials here.
