# Nix Noise Models and NixOS Module Design

## Scope

Make Lyre's Nix API package and NixOS deployment path usable with server-side noise cancellation without a manually populated working-directory model tree.

This increment covers:

- fixed-output Nix packaging for the server-side DeepFilterNet3 and DPDFNet ONNX model files that Lyre already knows how to load;
- a wrapped `lyre-api` package that supplies default model path environment variables pointing at the packaged model directories;
- a NixOS module for the Rust API service with built-in model path environment, persistent state/runtime directories, and optional `envFile` support;
- focused CLI test coverage proving model paths are configurable through environment variables.

This increment does not add Nix packaging for the Next.js frontend, Kubernetes changes, new model formats, runtime network model downloads, or new denoise provider behavior.

## Current State

Rust already accepts server model directory paths through these `lyre serve` inputs:

- `--deepfilternet-model-dir` / `LYRE_DEEPFILTERNET_MODEL_DIR`
- `--dpdfnet-model-dir` / `LYRE_DPDFNET_MODEL_DIR`

The default values are relative paths (`deepfilternet/onnx` and `dpdfnet/onnx`), so a plain Nix service currently starts without model files unless the operator separately provides matching directories. The flake packages only `lyre-api` and `lyre-debug`; it does not expose model packages or a NixOS module.

## Model Package

Add a Nix package named `lyre-noise-models`.

It must install this layout:

```text
$out/share/lyre/models/deepfilternet/onnx/enc.onnx
$out/share/lyre/models/deepfilternet/onnx/erb_dec.onnx
$out/share/lyre/models/deepfilternet/onnx/df_dec.onnx
$out/share/lyre/models/dpdfnet/onnx/baseline.onnx
$out/share/lyre/models/dpdfnet/onnx/dpdfnet2.onnx
$out/share/lyre/models/dpdfnet/onnx/dpdfnet4.onnx
$out/share/lyre/models/dpdfnet/onnx/dpdfnet8.onnx
$out/share/lyre/models/dpdfnet/onnx/dpdfnet2_48khz_hr.onnx
$out/share/lyre/models/dpdfnet/onnx/dpdfnet8_48khz_hr.onnx
```

The DeepFilterNet files come from `bitsydarel/deepfilternet3-onnx` on Hugging Face:

- `enc.onnx`, hash `sha256-fFOZ09qKUOvvHBoK5CGzM3aqXkXQ6S3xbafoPJwTGRY=`
- `erb_dec.onnx`, hash `sha256-q2aaHRCv4gkRcoszBTpFIHEEIxepBYEJKzJdp7L52JU=`
- `df_dec.onnx`, hash `sha256-IxFM47D2Rkt2PuYve7iqtrKhKaIeq9W8/llBPbBfJ4o=`

The DPDFNet files come from `Ceva-IP/DPDFNet` on Hugging Face under `onnx/`:

- `baseline.onnx`, hash `sha256-Nx0mGCr/Dh4NMTVOJMgfec1XRY9afdAD/BCn6/ZCVeA=`
- `dpdfnet2.onnx`, hash `sha256-Tw7iiTW0oyq+zHF9dFQWl2Vlg02DlgGs9DAxCUtNyUw=`
- `dpdfnet4.onnx`, hash `sha256-QqOG02AZIvR0xDCZJ+H0cczkIum9jj/Tlnu5Q7rDbAE=`
- `dpdfnet8.onnx`, hash `sha256-iZ1PI/P/hu2/+oxTfkvL3EnaG06E4O85BhHgYEo7Jss=`
- `dpdfnet2_48khz_hr.onnx`, hash `sha256-fwV1pc7Auk/9j4vWV+BtAH5MzdlV12+quSK50ykdwUs=`
- `dpdfnet8_48khz_hr.onnx`, hash `sha256-ezr7smCgj+mvPRbjvamSlxvh5+lR0d7nwtI19cQ/VjE=`

The package must expose a stable store path usable by systemd environment variables. It must not copy model files into the source tree or depend on ignored local directories.

## API Package Defaults

Wrap the `lyre-api` executable so Nix-installed `lyre serve` has model directory defaults without changing CLI precedence:

- default `LYRE_DEEPFILTERNET_MODEL_DIR` to `${lyre-noise-models}/share/lyre/models/deepfilternet/onnx` when the variable is unset;
- default `LYRE_DPDFNET_MODEL_DIR` to `${lyre-noise-models}/share/lyre/models/dpdfnet/onnx` when the variable is unset.

Explicit CLI flags and explicitly supplied environment variables must continue to take precedence. `lyre-debug` may keep using the wrapped `lyre` binary and must continue to inject `serve --enable-prof` for debug runs.

## NixOS Module

Expose `nixosModules.default` and `nixosModules.lyre` from the flake.

The module owns only the Rust API service and must provide:

- `services.lyre.enable`
- `services.lyre.package`, defaulting to this flake's `lyre-api` package for the host system
- `services.lyre.noiseModels.package`, defaulting to this flake's `lyre-noise-models` package for the host system
- `services.lyre.host`, default `127.0.0.1`
- `services.lyre.port`, default `8080`
- `services.lyre.environment`, an attribute set of extra environment variables
- `services.lyre.envFile`, optional path/string forwarded to systemd `EnvironmentFile`
- `services.lyre.stateDirectory`, default `lyre`
- `services.lyre.runtimeDirectory`, default `lyre`
- `services.lyre.workingDirectory`, default `/var/lib/lyre`

When enabled, the module must create `systemd.services.lyre` with:

- `after = [ "network.target" ]
- `wantedBy = [ "multi-user.target" ]`
- `Restart = "always"`
- `DynamicUser = true`
- `StateDirectory`, `RuntimeDirectory`, `WorkingDirectory`, and `StateDirectoryMode = "0700"`
- `ExecStart = "${cfg.package}/bin/lyre serve"`
- environment entries for `LYRE_HOST`, `LYRE_PORT`, `LYRE_DEEPFILTERNET_MODEL_DIR`, and `LYRE_DPDFNET_MODEL_DIR`
- the user-supplied `services.lyre.environment` merged after module defaults
- `EnvironmentFile = cfg.envFile` only when `envFile` is not null

The module must allow the user's `envFile` to carry secrets such as `LYRE_TURN_REST_SECRET` and ordinary runtime values, matching the provided systemd example.

## Documentation

Update deployment/runtime docs to show a minimal NixOS configuration equivalent to:

```nix
services.lyre = {
  enable = true;
  port = 8123;
  envFile = config.sops.templates."lyre-env".path;
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
```

Also document the packaged model paths and the fact that `lyre-api` sets model path defaults only when the corresponding environment variables are unset.

Update `docs/roadmap.md` after implementation, per repository guidance.

## Acceptance Criteria

- `nix build .#lyre-noise-models` produces the expected model directory layout.
- `nix build .#lyre-api` produces a `lyre` wrapper whose runtime closure includes `lyre-noise-models` and whose unset model path environment defaults point to the packaged model directories.
- `nix flake show` lists `nixosModules.default`, `nixosModules.lyre`, `packages.<system>.lyre-api`, and `packages.<system>.lyre-noise-models`.
- NixOS module evaluation for an enabled service includes the expected systemd environment, `EnvironmentFile` when configured, and `ExecStart` using the configured package.
- Rust CLI tests cover `LYRE_DPDFNET_MODEL_DIR` in addition to the existing DeepFilterNet env coverage.
- `cargo fmt`, `cargo clippy`, and `cargo nextest run --manifest-path "Cargo.toml" --workspace` pass after code changes.
