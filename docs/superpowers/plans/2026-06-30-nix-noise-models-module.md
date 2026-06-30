# Nix Noise Models and NixOS Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Package Lyre's server-side denoise ONNX models in Nix, make the Nix API package default to those model paths, and provide a DynamicUser-based NixOS service module with `envFile` support.

**Architecture:** Keep Rust model loading unchanged and use the existing CLI/env path boundary. Nix owns model acquisition through fixed-output fetches, wraps the API binary with unset-only model path defaults, and exposes a focused NixOS module for the Rust API service.

**Tech Stack:** Rust/clap tests, Nix flakes, nixpkgs `fetchurl`/`runCommand`/`makeWrapper`, NixOS module system, systemd.

## Global Constraints

- Do not expose real user domains or real public IPs in docs, specs, plans, examples, or tests; use `example.com` and reserved documentation IPs.
- The NixOS service must use `DynamicUser = true`; do not create or expose fixed `user`/`group` options.
- Frontend Nix packaging is out of scope.
- Keep runtime model downloads out of Rust; model acquisition belongs to Nix fixed-output derivations.
- Preserve CLI/env precedence: explicit CLI flags and explicitly supplied env vars override Nix wrapper defaults.
- Update `docs/roadmap.md` after code changes.
- Run `cargo fmt`, `cargo clippy`, and `cargo nextest run --manifest-path "Cargo.toml" --workspace` after edits.

---

## File Structure

- Modify `crates/lyre-app/src/cli/serve/tests/deepfilternet.rs`: add focused coverage for `LYRE_DPDFNET_MODEL_DIR`.
- Modify `flake.nix`: add fixed-output model fetches, `lyre-noise-models`, wrapped `lyre-api`, wrapped `lyre-debug`, Linux-only NixOS module eval check, and flake module outputs.
- Create `nix/modules/lyre.nix`: NixOS service module for `services.lyre`.
- Modify `docs/runtime-configuration.md`: document packaged model paths and env override behavior.
- Modify `docs/deployment.md`: document Nix build/package outputs and a sanitized NixOS module example.
- Modify `docs/roadmap.md`: record completed Nix denoise model packaging and NixOS module.

## Task 1: Add Missing DPDFNet Env Test

**Files:**
- Modify: `crates/lyre-app/src/cli/serve/tests/deepfilternet.rs`

**Interfaces:**
- Consumes: existing `ServeArgs::effective_noise_model_runtime()` and `EnvVarGuard`.
- Produces: regression coverage for `LYRE_DPDFNET_MODEL_DIR`.

- [ ] **Step 1: Add the failing env test**

Add this test after `parses_dpdfnet_model_dir_cli_arg`:

```rust
#[test]
fn dpdfnet_model_dir_env_enables_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _model_dir = EnvVarGuard::set("LYRE_DPDFNET_MODEL_DIR", "/env/dpdfnet");

    let cli = Cli::try_parse_from(["lyre", "serve"]).unwrap();

    match cli.command {
        Commands::Serve(args) => {
            let runtime = args.effective_noise_model_runtime().unwrap();
            assert_eq!(runtime.dpdfnet.model_dir, std::path::PathBuf::from("/env/dpdfnet"));
        }
        Commands::Config(_) => panic!("expected serve"),
    }
}
```

- [ ] **Step 2: Run the focused test**

Run: `cargo test -p lyre-app dpdfnet_model_dir_env_enables_config`

Expected: PASS because clap already declares `env = "LYRE_DPDFNET_MODEL_DIR"`.

## Task 2: Package Noise Models and Wrap API Binaries

**Files:**
- Modify: `flake.nix`

**Interfaces:**
- Consumes: existing `lyre-api` and `lyre-debug` package definitions.
- Produces: `packages.<system>.lyre-noise-models`; `lyre-api` and `lyre-debug` wrappers with unset-only model path defaults.

- [ ] **Step 1: Add `makeWrapper` to API native build inputs**

In `commonArgs.nativeBuildInputs`, add `pkgs.makeWrapper` beside the existing native build tools so package `postInstall` hooks can wrap `$out/bin/lyre`.

- [ ] **Step 2: Add fixed-output model fetches**

In the `let` block before `commonArgs`, add attribute sets for the DeepFilterNet and DPDFNet model files using these exact URLs and hashes:

```nix
deepfilternetModelFiles = {
  enc = pkgs.fetchurl {
    url = "https://huggingface.co/bitsydarel/deepfilternet3-onnx/resolve/main/enc.onnx";
    hash = "sha256-fFOZ09qKUOvvHBoK5CGzM3aqXkXQ6S3xbafoPJwTGRY=";
  };
  erb_dec = pkgs.fetchurl {
    url = "https://huggingface.co/bitsydarel/deepfilternet3-onnx/resolve/main/erb_dec.onnx";
    hash = "sha256-q2aaHRCv4gkRcoszBTpFIHEEIxepBYEJKzJdp7L52JU=";
  };
  df_dec = pkgs.fetchurl {
    url = "https://huggingface.co/bitsydarel/deepfilternet3-onnx/resolve/main/df_dec.onnx";
    hash = "sha256-IxFM47D2Rkt2PuYve7iqtrKhKaIeq9W8/llBPbBfJ4o=";
  };
};

dpdfnetModelFiles = {
  baseline = pkgs.fetchurl {
    url = "https://huggingface.co/Ceva-IP/DPDFNet/resolve/main/onnx/baseline.onnx";
    hash = "sha256-Nx0mGCr/Dh4NMTVOJMgfec1XRY9afdAD/BCn6/ZCVeA=";
  };
  dpdfnet2 = pkgs.fetchurl {
    url = "https://huggingface.co/Ceva-IP/DPDFNet/resolve/main/onnx/dpdfnet2.onnx";
    hash = "sha256-Tw7iiTW0oyq+zHF9dFQWl2Vlg02DlgGs9DAxCUtNyUw=";
  };
  dpdfnet4 = pkgs.fetchurl {
    url = "https://huggingface.co/Ceva-IP/DPDFNet/resolve/main/onnx/dpdfnet4.onnx";
    hash = "sha256-QqOG02AZIvR0xDCZJ+H0cczkIum9jj/Tlnu5Q7rDbAE=";
  };
  dpdfnet8 = pkgs.fetchurl {
    url = "https://huggingface.co/Ceva-IP/DPDFNet/resolve/main/onnx/dpdfnet8.onnx";
    hash = "sha256-iZ1PI/P/hu2/+oxTfkvL3EnaG06E4O85BhHgYEo7Jss=";
  };
  dpdfnet2_48khz_hr = pkgs.fetchurl {
    url = "https://huggingface.co/Ceva-IP/DPDFNet/resolve/main/onnx/dpdfnet2_48khz_hr.onnx";
    hash = "sha256-fwV1pc7Auk/9j4vWV+BtAH5MzdlV12+quSK50ykdwUs=";
  };
  dpdfnet8_48khz_hr = pkgs.fetchurl {
    url = "https://huggingface.co/Ceva-IP/DPDFNet/resolve/main/onnx/dpdfnet8_48khz_hr.onnx";
    hash = "sha256-ezr7smCgj+mvPRbjvamSlxvh5+lR0d7nwtI19cQ/VjE=";
  };
};
```

- [ ] **Step 3: Add `lyre-noise-models` derivation**

Add:

```nix
lyre-noise-models = pkgs.runCommand "lyre-noise-models" { } ''
  install -Dm0644 ${deepfilternetModelFiles.enc} "$out/share/lyre/models/deepfilternet/onnx/enc.onnx"
  install -Dm0644 ${deepfilternetModelFiles.erb_dec} "$out/share/lyre/models/deepfilternet/onnx/erb_dec.onnx"
  install -Dm0644 ${deepfilternetModelFiles.df_dec} "$out/share/lyre/models/deepfilternet/onnx/df_dec.onnx"
  install -Dm0644 ${dpdfnetModelFiles.baseline} "$out/share/lyre/models/dpdfnet/onnx/baseline.onnx"
  install -Dm0644 ${dpdfnetModelFiles.dpdfnet2} "$out/share/lyre/models/dpdfnet/onnx/dpdfnet2.onnx"
  install -Dm0644 ${dpdfnetModelFiles.dpdfnet4} "$out/share/lyre/models/dpdfnet/onnx/dpdfnet4.onnx"
  install -Dm0644 ${dpdfnetModelFiles.dpdfnet8} "$out/share/lyre/models/dpdfnet/onnx/dpdfnet8.onnx"
  install -Dm0644 ${dpdfnetModelFiles.dpdfnet2_48khz_hr} "$out/share/lyre/models/dpdfnet/onnx/dpdfnet2_48khz_hr.onnx"
  install -Dm0644 ${dpdfnetModelFiles.dpdfnet8_48khz_hr} "$out/share/lyre/models/dpdfnet/onnx/dpdfnet8_48khz_hr.onnx"
'';
```

- [ ] **Step 4: Add shared wrapper hook**

Add a `wrapLyreWithNoiseModels` string in the `let` block:

```nix
deepfilternetModelDir = "${lyre-noise-models}/share/lyre/models/deepfilternet/onnx";
dpdfnetModelDir = "${lyre-noise-models}/share/lyre/models/dpdfnet/onnx";
wrapLyreWithNoiseModels = ''
  wrapProgram "$out/bin/lyre" \
    --set-default LYRE_DEEPFILTERNET_MODEL_DIR "${deepfilternetModelDir}" \
    --set-default LYRE_DPDFNET_MODEL_DIR "${dpdfnetModelDir}"
'';
```

- [ ] **Step 5: Wrap `lyre-api` and `lyre-debug`**

Set `postInstall = wrapLyreWithNoiseModels;` on `lyre-api`.

For `lyre-debug`, prepend the same wrapper hook before creating `lyre-debug`:

```nix
postInstall = wrapLyreWithNoiseModels + ''
  wrapper="$out/bin/lyre-debug"
  ... existing wrapper body ...
'';
```

- [ ] **Step 6: Export the model package**

In `packages`, include `lyre-noise-models`:

```nix
packages = {
  default = lyre-api;
  inherit lyre-api lyre-debug lyre-noise-models;
};
```

- [ ] **Step 7: Verify model package and wrappers**

Run:

```bash
nix build .#lyre-noise-models
test -f result/share/lyre/models/deepfilternet/onnx/enc.onnx
test -f result/share/lyre/models/dpdfnet/onnx/dpdfnet2_48khz_hr.onnx
nix build .#lyre-api
rg 'LYRE_DEEPFILTERNET_MODEL_DIR|LYRE_DPDFNET_MODEL_DIR|lyre-noise-models' result/bin/lyre
```

Expected: all commands exit 0, and `rg` shows wrapper defaults referencing `lyre-noise-models`.

## Task 3: Add NixOS Module

**Files:**
- Create: `nix/modules/lyre.nix`
- Modify: `flake.nix`

**Interfaces:**
- Consumes: `self.packages.${system}.lyre-api` and `self.packages.${system}.lyre-noise-models`.
- Produces: `nixosModules.default`, `nixosModules.lyre`, and a Linux-only module eval check.

- [ ] **Step 1: Create the module file**

Create `nix/modules/lyre.nix`:

```nix
{ self }:

{ config, lib, pkgs, ... }:

let
  cfg = config.services.lyre;
  packages = self.packages.${pkgs.stdenv.hostPlatform.system};
  modelRoot = "${cfg.noiseModels.package}/share/lyre/models";
in
{
  options.services.lyre = {
    enable = lib.mkEnableOption "Lyre Rust API service";

    package = lib.mkOption {
      type = lib.types.package;
      default = packages.lyre-api;
      defaultText = lib.literalExpression "self.packages.\${pkgs.stdenv.hostPlatform.system}.lyre-api";
      description = "Lyre API package to run.";
    };

    noiseModels.package = lib.mkOption {
      type = lib.types.package;
      default = packages.lyre-noise-models;
      defaultText = lib.literalExpression "self.packages.\${pkgs.stdenv.hostPlatform.system}.lyre-noise-models";
      description = "Packaged ONNX denoise models used by the Lyre API service.";
    };

    host = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1";
      description = "Host address for the Lyre API listener.";
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 8080;
      description = "TCP port for the Lyre API listener.";
    };

    environment = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      description = "Extra environment variables for the Lyre service.";
    };

    envFile = lib.mkOption {
      type = lib.types.nullOr (lib.types.either lib.types.path lib.types.str);
      default = null;
      description = "Optional systemd EnvironmentFile path for secrets or deployment-specific settings.";
    };

    stateDirectory = lib.mkOption {
      type = lib.types.str;
      default = "lyre";
      description = "systemd StateDirectory for Lyre.";
    };

    runtimeDirectory = lib.mkOption {
      type = lib.types.str;
      default = "lyre";
      description = "systemd RuntimeDirectory for Lyre.";
    };

    workingDirectory = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/lyre";
      description = "Working directory for the Lyre process.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.services.lyre = {
      after = [ "network.target" ];
      wantedBy = [ "multi-user.target" ];

      environment = {
        LYRE_HOST = cfg.host;
        LYRE_PORT = toString cfg.port;
        LYRE_DEEPFILTERNET_MODEL_DIR = "${modelRoot}/deepfilternet/onnx";
        LYRE_DPDFNET_MODEL_DIR = "${modelRoot}/dpdfnet/onnx";
      } // cfg.environment;

      serviceConfig = {
        Restart = "always";
        DynamicUser = true;
        StateDirectory = cfg.stateDirectory;
        RuntimeDirectory = cfg.runtimeDirectory;
        WorkingDirectory = cfg.workingDirectory;
        StateDirectoryMode = "0700";
        ExecStart = "${cfg.package}/bin/lyre serve";
      } // lib.optionalAttrs (cfg.envFile != null) {
        EnvironmentFile = cfg.envFile;
      };
    };
  };
}
```

- [ ] **Step 2: Export module outputs**

Add a top-level merge around `flake-utils.lib.eachDefaultSystem` so the flake also exports modules:

```nix
  }:
    flake-utils.lib.eachDefaultSystem (...existing per-system body...)
    // {
      nixosModules.default = self.nixosModules.lyre;
      nixosModules.lyre = import ./nix/modules/lyre.nix { inherit self; };
    };
```

Keep the existing per-system outputs unchanged except for package/check additions.

- [ ] **Step 3: Add Linux-only module eval check**

Inside per-system `checks`, merge this for Linux systems:

```nix
// pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
  lyre-nixos-module =
    let
      evaluated = nixpkgs.lib.nixosSystem {
        inherit system;
        modules = [
          self.nixosModules.lyre
          {
            services.lyre = {
              enable = true;
              port = 8123;
              envFile = "/run/secrets/lyre-env";
              environment.LYRE_CORS_ALLOWED_ORIGINS = "https://example.com";
            };
          }
        ];
      };
      service = evaluated.config.systemd.services.lyre;
      env = service.environment;
      serviceConfig = service.serviceConfig;
    in
    assert env.LYRE_HOST == "127.0.0.1";
    assert env.LYRE_PORT == "8123";
    assert env.LYRE_CORS_ALLOWED_ORIGINS == "https://example.com";
    assert env.LYRE_DEEPFILTERNET_MODEL_DIR == "${lyre-noise-models}/share/lyre/models/deepfilternet/onnx";
    assert env.LYRE_DPDFNET_MODEL_DIR == "${lyre-noise-models}/share/lyre/models/dpdfnet/onnx";
    assert serviceConfig.DynamicUser == true;
    assert serviceConfig.EnvironmentFile == "/run/secrets/lyre-env";
    assert serviceConfig.ExecStart == "${lyre-api}/bin/lyre serve";
    pkgs.runCommand "lyre-nixos-module-check" { } ''
      touch "$out"
    '';
}
```

- [ ] **Step 4: Verify flake/module outputs**

Run:

```bash
nix flake show --json | rg 'lyre-noise-models|nixosModules|lyre-nixos-module'
nix build .#checks.x86_64-linux.lyre-nixos-module
```

Expected: output lists the new package/module/check, and the module check builds on Linux.

## Task 4: Documentation and Roadmap

**Files:**
- Modify: `docs/runtime-configuration.md`
- Modify: `docs/deployment.md`
- Modify: `docs/roadmap.md`

**Interfaces:**
- Consumes: final Nix package/module names and sanitized example values.
- Produces: operator docs without real domains or real public IPs.

- [ ] **Step 1: Document packaged model defaults**

In `docs/runtime-configuration.md`, extend the DeepFilterNet/DPDFNet runtime sections to mention:

```markdown
The Nix `lyre-api` package sets `LYRE_DEEPFILTERNET_MODEL_DIR` and `LYRE_DPDFNET_MODEL_DIR` to packaged store paths when those variables are unset. Explicit CLI flags or environment variables still override those defaults.

The packaged model directories are:

- `${lyre-noise-models}/share/lyre/models/deepfilternet/onnx`
- `${lyre-noise-models}/share/lyre/models/dpdfnet/onnx`
```

- [ ] **Step 2: Add NixOS deployment example**

In `docs/deployment.md`, add a Nix/NixOS section. It must include the commands `nix build .#lyre-api` and `nix build .#lyre-noise-models`, then this sanitized module example:

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

Also state that `envFile` maps to systemd `EnvironmentFile` and is intended for secrets such as `LYRE_TURN_REST_SECRET`, and that the service uses `DynamicUser = true` with `StateDirectory = "lyre"`.

- [ ] **Step 3: Update roadmap**

Add a Completed bullet to `docs/roadmap.md`:

```markdown
- Nix packaging now includes fixed-output DeepFilterNet3 and DPDFNet ONNX model derivations, unset-only API wrapper defaults for model paths, and a DynamicUser-based NixOS module with `envFile` support.
```

- [ ] **Step 4: Check docs for real domains/IPs**

Run:

```bash
rg -n 'indexyz|101\.34\.|[a-zA-Z0-9.-]+\.me' docs README.md flake.nix nix crates frontend scripts .github
```

Expected: no new real-domain examples from this task. Existing unrelated occurrences, if any, must be evaluated before final reporting.

## Task 5: Final Verification

**Files:**
- No new files beyond earlier tasks.

**Interfaces:**
- Consumes: all implemented changes.
- Produces: evidence for completion, review, commit, and push.

- [ ] **Step 1: Format Rust and Nix**

Run:

```bash
cargo fmt
git diff --check
```

Expected: Rust formatting completes successfully and whitespace check passes.

- [ ] **Step 2: Run Rust verification**

Run:

```bash
cargo clippy
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: both pass.

- [ ] **Step 3: Run Nix verification**

Run:

```bash
nix flake check
nix build .#lyre-api
nix build .#lyre-noise-models
```

Expected: all pass.

- [ ] **Step 4: Inspect final diff**

Run:

```bash
git status --short
git diff --check
git diff --stat
```

Expected: only intended files changed; whitespace check passes.
