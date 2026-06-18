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

        commonArgs = {
          inherit src;
          strictDeps = true;
          nativeBuildInputs = [
            pkgs.autoPatchelfHook
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
            postInstall = ''
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
          inherit lyre-api lyre-debug;
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
    );
}
