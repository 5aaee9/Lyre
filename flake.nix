{
  description = "Lyre Rust API build and development shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    flake-utils.url = "github:numtide/flake-utils";

    crane.url = "github:ipetkov/crane";

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      crane,
      fenix,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        fenixPkgs = fenix.packages.${system};

        toolchain = fenixPkgs.combine [
          (fenixPkgs.stable.withComponents [
            "cargo"
            "clippy"
            "rust-src"
            "rust-std"
            "rustc"
            "rustfmt"
          ])
          fenixPkgs.targets.wasm32-unknown-unknown.stable.rust-std
        ];

        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

        src = craneLib.cleanCargoSource ./.;
        apiCargoArgs = "-p lyre-app --bin lyre";

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

        deepfilternetModelDir = "${lyre-noise-models}/share/lyre/models/deepfilternet/onnx";
        dpdfnetModelDir = "${lyre-noise-models}/share/lyre/models/dpdfnet/onnx";
        wrapLyreWithNoiseModels = ''
          wrapProgram "$out/bin/lyre" \
            --set-default LYRE_DEEPFILTERNET_MODEL_DIR "${deepfilternetModelDir}" \
            --set-default LYRE_DPDFNET_MODEL_DIR "${dpdfnetModelDir}"
        '';

        commonArgs = {
          inherit src;
          strictDeps = true;
          nativeBuildInputs = [
            pkgs.autoPatchelfHook
            pkgs.makeWrapper
            pkgs.pkg-config
          ];
          buildInputs = [
            pkgs.libopus
            pkgs.onnxruntime
            pkgs.openssl
            pkgs.stdenv.cc.cc.lib
          ];
          runtimeDependencies = [
            pkgs.libopus
            pkgs.onnxruntime
            pkgs.openssl
            pkgs.stdenv.cc.cc.lib
          ];
          ORT_LIB_PATH = "${pkgs.onnxruntime}/lib";
          ORT_PREFER_DYNAMIC_LINK = "true";
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
            pkgs.libopus
            pkgs.onnxruntime
            pkgs.openssl
            pkgs.stdenv.cc.cc.lib
          ];
        };

        apiArgs = commonArgs // {
          cargoExtraArgs = apiCargoArgs;
        };

        cargoArtifacts = craneLib.buildDepsOnly (
          apiArgs
          // {
            pname = "lyre-api-deps";
          }
        );

        lyre-api = craneLib.buildPackage (
          apiArgs
          // {
            inherit cargoArtifacts;
            pname = "lyre-api";
            postInstall = wrapLyreWithNoiseModels;
          }
        );

        debugArgs = apiArgs // {
          CARGO_PROFILE_RELEASE_DEBUG = "true";
          RUSTFLAGS = "-C force-frame-pointers=yes";
        };

        lyre-debug = craneLib.buildPackage (
          debugArgs
          // {
            inherit cargoArtifacts;
            pname = "lyre-debug";
            dontStrip = true;
            postInstall = wrapLyreWithNoiseModels + ''
              wrapper="$out/bin/lyre-debug"
              printf '%s\n' \
                '#!${pkgs.runtimeShell}' \
                'set -eu' \
                "" \
                'if [ "''${1:-}" = "serve" ]; then' \
                '  shift' \
                '  exec "$(dirname "$0")/lyre" serve --enable-prof "$@"' \
                'fi' \
                "" \
                'exec "$(dirname "$0")/lyre" "$@"' \
                > "$wrapper"
              chmod +x "$wrapper"
            '';
            meta.mainProgram = "lyre-debug";
          }
        );
      in
      {
        packages = {
          default = lyre-api;
          inherit lyre-api lyre-debug lyre-noise-models;
        };

        checks = {
          inherit lyre-api;

          lyre-api-clippy = craneLib.cargoClippy (
            apiArgs
            // {
              inherit cargoArtifacts;
              pname = "lyre-api-clippy";
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            }
          );

          lyre-api-nextest = craneLib.cargoNextest (
            apiArgs
            // {
              inherit cargoArtifacts;
              pname = "lyre-api-nextest";
              cargoNextestExtraArgs = "--workspace";
            }
          );

          lyre-api-fmt = craneLib.cargoFmt {
            inherit src;
            pname = "lyre-api-fmt";
          };
        }
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
            assert env.LYRE_DEEPFILTERNET_MODEL_DIR == "${deepfilternetModelDir}";
            assert env.LYRE_DPDFNET_MODEL_DIR == "${dpdfnetModelDir}";
            assert serviceConfig.DynamicUser == true;
            assert serviceConfig.EnvironmentFile == "/run/secrets/lyre-env";
            assert serviceConfig.ExecStart == "${lyre-api}/bin/lyre serve";
            pkgs.runCommand "lyre-nixos-module-check" { } ''
              touch "$out"
            '';
        };

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};

          packages = with pkgs; [
            cargo-nextest
            toolchain
            fenixPkgs.rust-analyzer
            libopus
            onnxruntime
            openssl
            pkg-config
          ];

          LD_LIBRARY_PATH = commonArgs.LD_LIBRARY_PATH;
          ORT_LIB_PATH = commonArgs.ORT_LIB_PATH;
          ORT_PREFER_DYNAMIC_LINK = commonArgs.ORT_PREFER_DYNAMIC_LINK;
        };

        formatter = pkgs.nixfmt;
      }
    )
    // {
      nixosModules.default = self.nixosModules.lyre;
      nixosModules.lyre = import ./nix/modules/lyre.nix { inherit self; };
    };
}
