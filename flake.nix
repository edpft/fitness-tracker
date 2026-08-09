{
  description = "Fitness tracker: personal health and fitness data, ingested and analysed";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    crane.url = "github:ipetkov/crane";

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      flake-utils,
      fenix,
      advisory-db,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          config = {
            allowUnfree = true;
          };
        };

        inherit (pkgs) lib;

        rustToolchain = fenix.packages.${system}.fromToolchainFile {
          file = ./rust-toolchain.toml;
          # Changing rust-toolchain.toml invalidates this: set it to
          # `lib.fakeHash`, build, and paste the hash nix reports back here.
          sha256 = "sha256-gh/xTkxKHL4eiRXzWv8KP7vfjSk61Iq48x47BEDFgfk=";
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        src = craneLib.cleanCargoSource ./.;

        buildDeps = (
          with pkgs;
          [
            # Faster linking with mold
            clang
            mold
          ]
        );

        commonArgs = {
          inherit src;
          strictDeps = true;

          nativeBuildInputs = buildDeps;

          buildInputs = [
            # TODO: runtime/link-time deps
          ]
          ++ lib.optionals pkgs.stdenv.isDarwin [
            pkgs.libiconv
          ];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        individualCrateArgs = commonArgs // {
          inherit cargoArtifacts;
          inherit (craneLib.crateNameFromCargoToml { inherit src; }) version;
          # NB: we disable tests since we'll run them all via cargo-nextest
          doCheck = false;
        };

        # Every crate builds from the same file set: cargo needs the whole
        # workspace manifest graph even when building a single member.
        workspaceSrc = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            # One entry per workspace member:
            (craneLib.fileset.commonCargoSources ./crates/domain)
            (craneLib.fileset.commonCargoSources ./crates/application)
            (craneLib.fileset.commonCargoSources ./crates/infrastructure)
            (craneLib.fileset.commonCargoSources ./crates/web)
            # Tool config files — add as you create them:
            # ./clippy.toml
            # ./rustfmt.toml
            ./deny.toml
            # ./taplo.toml
          ];
        };

        # Add one of these per workspace member you want to build.
        domain = craneLib.buildPackage (
          individualCrateArgs
          // {
            cargoExtraArgs = "-p domain";
            src = workspaceSrc;
          }
        );

        application = craneLib.buildPackage (
          individualCrateArgs
          // {
            cargoExtraArgs = "-p application";
            src = workspaceSrc;
          }
        );

        infrastructure = craneLib.buildPackage (
          individualCrateArgs
          // {
            cargoExtraArgs = "-p infrastructure";
            src = workspaceSrc;
          }
        );

        web = craneLib.buildPackage (
          individualCrateArgs
          // {
            cargoExtraArgs = "-p web";
            src = workspaceSrc;
            # Lets `nix run` find the binary without a hand-written app.
            meta.mainProgram = "web";
          }
        );

      in
      {
        checks = {
          inherit
            domain
            application
            infrastructure
            web
            ;

          format = craneLib.cargoFmt {
            inherit src;
          };

          lint = craneLib.cargoClippy (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            }
          );

          test = craneLib.cargoNextest (
            commonArgs
            // {
              inherit cargoArtifacts;
            }
          );

          # nextest does not run doctests, so they need their own check.
          doctest = craneLib.cargoDocTest (
            commonArgs
            // {
              inherit cargoArtifacts;
            }
          );

          nix-format =
            pkgs.runCommand "nix-format"
              {
                nativeBuildInputs = [ pkgs.nixfmt ];
              }
              ''
                find ${
                  lib.fileset.toSource {
                    root = ./.;
                    fileset = lib.fileset.fileFilter (file: file.hasExt "nix") ./.;
                  }
                } -name '*.nix' -exec nixfmt --check {} +
                touch "$out"
              '';

          # nixpkgs' actionlint brings shellcheck with it, so `run:` scripts
          # are linted too, not just the workflow schema.
          workflows =
            pkgs.runCommand "workflows"
              {
                nativeBuildInputs = [ pkgs.actionlint ];
              }
              ''
                actionlint ${
                  lib.fileset.toSource {
                    root = ./.;
                    fileset = ./.github/workflows;
                  }
                }/.github/workflows/*.yml
                touch "$out"
              '';

          audit-deps = craneLib.cargoAudit {
            inherit src advisory-db;
          };

          audit-licenses = craneLib.cargoDeny {
            inherit src;
          };
        };

        packages.default = web;

        formatter = pkgs.nixfmt;

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};
          packages = with pkgs; [
            # The tools CI uses, so a failure can be reproduced locally.
            cargo-audit
            cargo-deny
            cargo-nextest
            nixfmt
            # Conveniences.
            actionlint
            cargo-watch
            taplo
            # For spec-kit
            uv
          ];
        };
      }
    );

}
