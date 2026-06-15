# Docker and Deployment

## Docker Images

Build local images:

```bash
docker build --target api -t lyre-api:local .
docker build --target web -t lyre-web:local .
```

`lyre-api` serves REST, WebSocket, WebRPC, and metrics endpoints on port `8080`.

`lyre-web` serves the Next.js app on port `3000` and expects:

- `APP_BASE_URL` for the public frontend URL.
- `APP_API_URL` for the Rust API URL.

The Next server injects those values into browser runtime config.

## Kubernetes

The repository includes Helm chart and Kubernetes assets for deploying `lyre-api` and `lyre-web`, with optional Ingress and Gateway API HTTPRoute entry points.

See `scripts/kubernetes/README.md` for Kubernetes-specific commands.
