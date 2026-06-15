# Lyre Helm Chart

This directory contains a Helm chart for running Lyre as two Kubernetes
workloads:

- `lyre-api`: Rust Axum REST/WebSocket/WebRPC API on port `8080`.
- `lyre-web`: Next.js standalone frontend on port `3000`.

Install or upgrade the release:

```bash
helm upgrade --install lyre scripts/kubernetes --namespace lyre --create-namespace
```

Or use the helper:

```bash
scripts/kubernetes/deploy.sh
```

Set image tags:

```bash
helm upgrade --install lyre scripts/kubernetes \
  --namespace lyre \
  --create-namespace \
  --set api.image.tag=<tag> \
  --set web.image.tag=<tag>
```

Default image repositories are:

- `ghcr.io/5aaee9/lyre/lyre-api`
- `ghcr.io/5aaee9/lyre/lyre-web`

By default, neither Ingress nor Gateway API routes are enabled. The chart only
creates ClusterIP services unless an entry point is explicitly enabled.

Enable Kubernetes Ingress:

```bash
helm upgrade --install lyre scripts/kubernetes \
  --namespace lyre \
  --create-namespace \
  --set ingress.enabled=true \
  --set ingress.host=voice.example.com \
  --set web.env.APP_BASE_URL=https://voice.example.com \
  --set web.env.APP_API_URL=https://voice.example.com
```

Enable Gateway API HTTPRoute against an existing Gateway:

```bash
helm upgrade --install lyre scripts/kubernetes \
  --namespace lyre \
  --create-namespace \
  --set gateway.enabled=true \
  --set gateway.parentRefs[0].name=public-gateway \
  --set gateway.parentRefs[0].namespace=gateway-system \
  --set gateway.parentRefs[0].sectionName=https \
  --set gateway.hostnames[0]=voice.example.com \
  --set web.env.APP_BASE_URL=https://voice.example.com \
  --set web.env.APP_API_URL=https://voice.example.com
```

Both Ingress and Gateway API routes send `/api`, `/rpc`, `/health`, and
`/metrics` to `lyre-api`; all other paths route to `lyre-web`.

Embedded TURN is disabled by default. Enable it only when your cluster can expose
the required UDP ports:

```bash
helm upgrade --install lyre scripts/kubernetes \
  --namespace lyre \
  --create-namespace \
  --set embeddedTurn.enabled=true \
  --set embeddedTurn.env.LYRE_TURN_REST_SECRET=<shared-secret> \
  --set embeddedTurn.env.LYRE_EMBEDDED_TURN_EXTERNAL=<public-ip-or-host>:3478
```

For production, a dedicated TURN service such as coturn or a cloud TURN provider
is usually easier to operate than exposing a large UDP relay port range from the
Lyre API pod.

Package and publish the chart as an OCI artifact:

```bash
helm package scripts/kubernetes --destination /tmp
helm push /tmp/lyre-0.1.0.tgz oci://ghcr.io/5aaee9/lyre/chart
```

Install from GHCR:

```bash
helm upgrade --install lyre oci://ghcr.io/5aaee9/lyre/chart/lyre \
  --version 0.1.0 \
  --namespace lyre \
  --create-namespace
```

The GitHub Actions workflow publishes the same chart to
`ghcr.io/5aaee9/lyre/chart/lyre`. Tag builds use the tag version without the leading
`v`; main branch builds do not publish Helm charts.
