# Appliance migration

The target architecture contains no Django component. It runs a FastAPI `control-api` container and a React Admin `web-console` container.

1. Recreate or reset the Ubuntu 24.04 VM through the provider, preserving the `nestbox_builder.pub` key as the `nestbox` user's authorized key.
2. Run `bootstrap-appliance.sh` as root on the new VM.
3. From this repository, run `NESTBOX_HOST=<new-ip> deploy/scripts/deploy-appliance.sh`.
4. Verify `http://<new-ip>:8080` and `curl http://127.0.0.1:8000/healthz` on the VM.

Do not put secrets in this repository, the bootstrap script, generated images, or seed bundles. Registry credentials must be introduced later through a dedicated secret-store integration.
