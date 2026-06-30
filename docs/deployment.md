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

## Nix

Build the Rust API and packaged denoise models:

```bash
nix build .#lyre-api
nix build .#lyre-noise-models
```

Use the NixOS module for the Rust API service:

```nix
{
  inputs.lyre.url = "github:example/lyre";

  outputs = { self, nixpkgs, lyre, ... }: {
    nixosConfigurations.example = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        lyre.nixosModules.default
        {
          services.lyre = {
            enable = true;
            port = 8123;
            envFile = "/run/secrets/lyre-env";
            environment = {
              LYRE_ENABLE_PROF = "true";
              LYRE_EMBEDDED_TURN = "true";
              LYRE_EMBEDDED_TURN_LISTEN = "0.0.0.0:3478";
              LYRE_EMBEDDED_TURN_EXTERNAL = "203.0.113.10:3478";
              LYRE_EMBEDDED_TURN_PORT_RANGE = "49152..65535";
              LYRE_SERVER_MEDIA_PUBLIC_IP = "203.0.113.10";
              LYRE_CORS_ALLOWED_ORIGINS = "https://example.com";
            };
          };
        }
      ];
    };
  };
}
```

`envFile` maps to systemd `EnvironmentFile` and is intended for secrets such as `LYRE_TURN_REST_SECRET`. The service uses `DynamicUser = true` with `StateDirectory = "lyre"`.

## Kubernetes

The repository includes Helm chart and Kubernetes assets for deploying `lyre-api` and `lyre-web`, with optional Ingress and Gateway API HTTPRoute entry points.

See `scripts/kubernetes/README.md` for Kubernetes-specific commands.
