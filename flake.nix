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

        toolchain = fenixPkgs.stable.toolchain;
        devToolchain = fenixPkgs.stable.withComponents [
          "cargo"
          "clippy"
          "rust-src"
          "rust-std"
          "rustc"
          "rustfmt"
        ];

        craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;

        src = craneLib.cleanCargoSource ./.;
        apiCargoArgs = "-p lyre-app --bin lyre";

        commonArgs = {
          inherit src;
          strictDeps = true;
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
      in
      {
        packages = {
          default = lyre-api;
          inherit lyre-api;
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
            devToolchain
            fenixPkgs.rust-analyzer
          ];
        };

        formatter = pkgs.nixfmt;
      }
    );
}
