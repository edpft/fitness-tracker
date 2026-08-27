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

        # Every crate builds from the same file set: cargo needs the whole
        # workspace manifest graph even when building a single member, and sqlx
        # needs the migrations and the offline query metadata at *compile*
        # time. `cleanCargoSource` would drop both — it keeps only files cargo
        # itself recognises — so the fileset is spelled out here and used
        # everywhere rather than only for the per-crate builds.
        src = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            # One entry per workspace member:
            (craneLib.fileset.commonCargoSources ./crates/domain)
            (craneLib.fileset.commonCargoSources ./crates/application)
            (craneLib.fileset.commonCargoSources ./crates/infrastructure)
            (craneLib.fileset.commonCargoSources ./crates/web)
            (craneLib.fileset.commonCargoSources ./crates/cli)
            # `sqlx::migrate!` embeds these, and `query!` is verified against
            # the schema they produce.
            ./migrations
            # The landed corpus, which the normalisation suites assert against.
            # `commonCargoSources` takes `.rs` and `Cargo.toml` and nothing
            # else, so a fixture in any other format has to be named here or
            # the tests find an empty file inside the sandbox.
            ./crates/infrastructure/tests/fixtures
            # Offline query metadata. Regenerate with `cargo sqlx prepare`
            # after changing a query; a stale directory fails the build here
            # rather than surprising someone later.
            ./.sqlx
            # Tool config files — add as you create them:
            ./clippy.toml
            # ./rustfmt.toml
            ./deny.toml
            ./taplo.toml
          ];
        };

        buildDeps = (
          with pkgs;
          [
            # Faster linking with mold
            clang
            mold
          ]
        );

        # **The version every derivation is named for, read once.**
        #
        # Not from the workspace root: it carries no version, because
        # release-please cannot maintain one there — its rust updater writes
        # `package.version` in each crate's own file and knows nothing about
        # `[workspace.package]`. A number nothing maintains is a number that
        # goes stale, so `versions-agree` refuses one there.
        #
        # Left to look in the root, crane finds nothing, warns, and names every
        # derivation `0.0.1` whatever has been released. `cli` is the crate that
        # ships the binary, and `linked-versions` makes its number every crate's.
        version = (craneLib.crateNameFromCargoToml { cargoToml = ./crates/cli/Cargo.toml; }).version;

        commonArgs = {
          inherit src version;
          pname = "workspace";
          strictDeps = true;

          # sqlx verifies every `query!` against the schema at compile time.
          # Offline means it reads the committed `.sqlx` metadata rather than
          # reaching for a database, which keeps the build hermetic — and makes
          # a query changed without regenerating that metadata a build failure.
          SQLX_OFFLINE = "true";

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
          # NB: we disable tests since we'll run them all via cargo-nextest
          doCheck = false;
        };

        # Everything under version control, for the checks that scan the
        # repository rather than build it.
        repoSrc = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.difference ./. (
            lib.fileset.unions (
              map lib.fileset.maybeMissing [
                ./target
                ./result
                ./.direnv
              ]
            )
          );
        };

        # Constitution § 16 and § 18: which ring each crate sits in. A crate may
        # only depend on a lower number. Adding a crate means deciding its ring,
        # which is the question worth being asked; what a crate takes from
        # crates.io is not this check's business — § 16 tags the vendor rule
        # `[review]`, and chrono or uuid in the domain is nobody's emergency.
        # `cli` and `web` are peers: two driving adapters, two composition
        # roots, neither depending on the other. Equal ring numbers make that
        # structural — the check requires a strict decrease across every edge,
        # so a dependency either way fails.
        crateRings = {
          domain = 0;
          application = 1;
          infrastructure = 2;
          web = 3;
          cli = 3;
        };

        # Add one of these per workspace member you want to build.
        domain = craneLib.buildPackage (
          individualCrateArgs
          // {
            cargoExtraArgs = "-p domain";
          }
        );

        application = craneLib.buildPackage (
          individualCrateArgs
          // {
            cargoExtraArgs = "-p application";
          }
        );

        infrastructure = craneLib.buildPackage (
          individualCrateArgs
          // {
            cargoExtraArgs = "-p infrastructure";
          }
        );

        web = craneLib.buildPackage (
          individualCrateArgs
          // {
            cargoExtraArgs = "-p web";
            # Lets `nix run` find the binary without a hand-written app.
            meta.mainProgram = "web";
          }
        );

        cli = craneLib.buildPackage (
          individualCrateArgs
          // {
            cargoExtraArgs = "-p cli";
            # The crate is `cli`; the binary operators type is `fitness`.
            meta.mainProgram = "fitness";
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
            cli
            ;

          format = craneLib.cargoFmt {
            inherit src version;
            pname = "workspace";
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

              # reqwest's TLS backend reads the platform trust store when a
              # client is *built*, and the sandbox has none — so constructing a
              # client fails even for a plain-HTTP request to a local stub, and
              # every contract test fails with "builder error".
              #
              # This grants no network access: the sandbox still has none, and
              # the tests still only talk to a stub on loopback. It only lets
              # the TLS backend initialise.
              #
              # Set here rather than in `commonArgs` on purpose. `commonArgs`
              # feeds `cargoArtifacts`, so putting it there would change the
              # hash of the vendored-dependency derivation and rebuild all ~300
              # crates. Only the tests run code that needs this.
              SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
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

          # Constitution § 16, § 18. See `crateRings` above.
          architecture =
            pkgs.runCommand "architecture"
              {
                nativeBuildInputs = [
                  rustToolchain
                  pkgs.jq
                ];
              }
              ''
                export CARGO_HOME="$TMPDIR/cargo"

                # The whole repository, not the curated build source: a crate
                # missing from `src` must still be seen here, or it
                # would evade the check entirely.
                cargo metadata --no-deps --offline --format-version 1 \
                  --manifest-path ${repoSrc}/Cargo.toml \
                  | jq -r '.packages[] | .name as $crate
                      | .dependencies[] | select(.path != null) | "\($crate) \(.name)"' \
                  | sort -u > edges

                printf '%s\n' ${
                  lib.escapeShellArgs (lib.mapAttrsToList (crate: ring: "${crate} ${toString ring}") crateRings)
                } > rings

                awk '
                  NR == FNR { ring[$1] = $2; next }
                  !($1 in ring) { print "  " $1 " is in no ring"; bad = 1; next }
                  !($2 in ring) { print "  " $2 " is in no ring"; bad = 1; next }
                  ring[$2] >= ring[$1] {
                    printf "  %s (ring %d) depends on %s (ring %d)\n", $1, ring[$1], $2, ring[$2]
                    bad = 1
                  }
                  END { exit bad }
                ' rings edges && exit_code=0 || exit_code=1

                if [ "$exit_code" -ne 0 ]; then
                  echo
                  echo "Constitution § 16: dependencies point inward only —"
                  echo "web -> infrastructure -> application -> domain. A crate"
                  echo "in no ring is a new one: give it a ring in flake.nix,"
                  echo "which is the decision worth making deliberately."
                  exit 1
                fi

                touch "$out"
              '';

          # Constitution § 15, § 16. The ring check above proves the
          # dependency points the right way; it cannot prove what is done with
          # it. `infrastructure` depends on `application` because that is where
          # the ports it implements are declared — and nothing in cargo stops a
          # driven adapter from reaching past those ports and calling a use
          # case, which would make the adapter drive the application it is
          # supposed to be driven by.
          #
          # `application` keeps its use cases behind `extract`, `normalise`,
          # `prescribe` and `status` rather than re-exporting them at the crate
          # root, so naming one requires naming its module. That makes this
          # greppable, which is the only reason the module boundary is worth
          # keeping. **All four are named here**: the check listed two of them
          # until 2026-08-19, so `prescribe` — added by feature 003 — was
          # unguarded for as long as it existed.
          #
          # Scoped to `src`, not the whole crate. An integration test at this ring
          # *does* drive a use case, and legitimately: the normalisation and
          # prescription suites need the real translator and the real store, and
          # `application` may not depend on the ring above it. So the rule is
          # about the adapter, and `tests/` is not the adapter.
          use-case-isolation = pkgs.runCommand "use-case-isolation" { } ''
            if grep -rn 'application::\(extract\|normalise\|prescribe\|status\)' \
                 ${repoSrc}/crates/infrastructure/src ${repoSrc}/crates/web/src; then
              echo
              echo "Constitution § 16: a driven adapter implements ports, it"
              echo "does not call the use cases. What infrastructure may name"
              echo "from application is its ports and its errors."
              exit 1
            fi

            touch "$out"
          '';

          # Constitution § 21. `toml` is the document format, and a document
          # format is an adapter's business: a `domain` type deserialised from
          # TOML is a domain shaped by a file format, and a `cli` that parses one
          # is a second reader to keep in step with the first.
          #
          # **The `architecture` check above does not catch this and cannot.** It
          # reads path dependencies only — that is what makes it a ring check —
          # and `toml` is a registry crate, so it is invisible there. Tasks.md
          # claimed otherwise, which is how a guard nobody had gets relied on.
          document-format-is-an-adapters =
            pkgs.runCommand "document-format"
              {
                nativeBuildInputs = [
                  rustToolchain
                  pkgs.jq
                ];
              }
              ''
                export CARGO_HOME="$TMPDIR/cargo"

                cargo metadata --no-deps --offline --format-version 1 \
                  --manifest-path ${repoSrc}/Cargo.toml \
                  | jq -r '.packages[] | .name as $crate
                      | .dependencies[] | select(.name == "toml") | $crate' \
                  | sort -u > readers

                if grep -v '^infrastructure$' readers; then
                  echo
                  echo "Constitution § 21: only infrastructure reads the document"
                  echo "format. A domain type deserialised from TOML is a domain"
                  echo "shaped by a file format."
                  exit 1
                fi

                touch "$out"
              '';

          # Not constitutional — build hygiene. `src` lists its members
          # by hand, so a new crate can be perfectly valid to cargo while nix
          # silently ignores its sources, and every per-crate build keeps
          # passing without ever compiling it.
          workspace-members =
            pkgs.runCommand "workspace-members"
              {
                nativeBuildInputs = [
                  rustToolchain
                  pkgs.jq
                ];
              }
              ''
                export CARGO_HOME="$TMPDIR/cargo"

                members() {
                  cargo metadata --no-deps --offline --format-version 1 \
                    --manifest-path "$1/Cargo.toml" \
                    | jq -r '.packages[].name' | sort
                }

                if ! diff -u <(members ${repoSrc}) <(members ${src}); then
                  echo
                  echo "A workspace member is missing from \`src\` in"
                  echo "flake.nix, so nix is not building it. Add it there, and"
                  echo "give it a \`buildPackage\` block if it should build alone."
                  exit 1
                fi

                touch "$out"
              '';

          # Not constitutional — release hygiene. The version is written in ten
          # places: each crate's `Cargo.toml` and each entry in
          # `.release-please-manifest.json`. The `linked-versions` plugin means
          # all ten are one number, and nothing enforced that — so a hand-edit,
          # or a release half-applied and half-reverted, would show up as a tag
          # that means nothing.
          #
          # **And the workspace root must not carry one.** Cargo's own
          # unification is `version.workspace = true` inheriting from
          # `[workspace.package]`, and release-please cannot do it: its rust
          # updater replaces `package.version` in each crate's own file and
          # throws on a value that is a table rather than a string
          # (`replaceTomlValue`: "value at path package.version is not
          # tagged"). So a `version` under `[workspace.package]` is a number
          # nothing maintains, sitting exactly where a reader looks first. It
          # was there and inert until 2026-08-27.
          versions-agree =
            pkgs.runCommand "versions-agree"
              {
                nativeBuildInputs = [
                  rustToolchain
                  pkgs.jq
                ];
              }
              ''
                export CARGO_HOME="$TMPDIR/cargo"

                cargo metadata --no-deps --offline --format-version 1 \
                  --manifest-path ${repoSrc}/Cargo.toml \
                  | jq -r '.packages[] | .name + " " + .version' | sort > cargo-says

                jq -r 'to_entries[] | (.key | split("/") | last) + " " + .value' \
                  ${repoSrc}/.release-please-manifest.json | sort > manifest-says

                if ! diff -u cargo-says manifest-says; then
                  echo
                  echo "A crate's version and its entry in"
                  echo ".release-please-manifest.json disagree. Release-please"
                  echo "writes both; if you are here after editing one by hand,"
                  echo "edit the other."
                  exit 1
                fi

                if [ "$(cut -d' ' -f2 cargo-says | sort -u | wc -l)" -ne 1 ]; then
                  echo
                  echo "The crates are on different versions. The"
                  echo "\`linked-versions\` plugin in release-please-config.json"
                  echo "means they release together and so share one number."
                  exit 1
                fi

                if awk '
                  /^\[workspace\.package\]/ { inside = 1; next }
                  /^\[/                       { inside = 0 }
                  inside && /^[[:space:]]*version[[:space:]]*=/ { found = 1 }
                  END { exit !found }
                ' ${repoSrc}/Cargo.toml; then
                  echo
                  echo "\`[workspace.package]\` in Cargo.toml carries a"
                  echo "\`version\`. Release-please does not maintain it and no"
                  echo "crate inherits it, so it is a number that can only go"
                  echo "stale. Remove it; the crates carry their own."
                  exit 1
                fi

                touch "$out"
              '';

          # Constitution § 35: credentials never enter version control. This
          # scans what is tracked; the structural defence is that credentials
          # come from the environment or an untracked local file, and the
          # scanner is the backstop for when that is forgotten.
          secrets =
            pkgs.runCommand "secrets"
              {
                nativeBuildInputs = [ pkgs.gitleaks ];
              }
              ''
                gitleaks dir --no-banner --redact --exit-code 1 ${repoSrc}
                touch "$out"
              '';

          typos =
            pkgs.runCommand "typos"
              {
                nativeBuildInputs = [ pkgs.typos ];
              }
              ''
                typos ${
                  lib.fileset.toSource {
                    root = ./.;
                    fileset = ./.;
                  }
                }
                touch "$out"
              '';

          unused-deps =
            pkgs.runCommand "unused-deps"
              {
                nativeBuildInputs = [ pkgs.cargo-machete ];
              }
              ''
                cargo-machete ${
                  lib.fileset.toSource {
                    root = ./.;
                    fileset = ./.;
                  }
                }
                touch "$out"
              '';

          toml-fmt = craneLib.taploFmt {
            inherit version;
            pname = "workspace";
            src = pkgs.lib.sources.sourceFilesBySuffices src [ ".toml" ];
          };

          audit-deps = craneLib.cargoAudit {
            inherit src advisory-db version;
            pname = "workspace";
          };

          audit-licenses = craneLib.cargoDeny {
            inherit src version;
            pname = "workspace";
          };
        };

        packages = {
          # `cli` is the default because it is the one that does something:
          # `nix run` should hand you the working tool, not the dormant HTTP
          # stub. Both are exposed by name.
          default = cli;
          inherit cli web;
        };

        # Takes the files to format: `nix fmt .`, not a bare `nix fmt`.
        formatter = pkgs.nixfmt;

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};
          packages = with pkgs; [
            # The tools CI uses, so a failure can be reproduced locally.
            cargo-audit
            cargo-deny
            cargo-nextest
            gitleaks
            nixfmt
            # Regenerating `.sqlx` after a query changes, and reading the store
            # by hand during validation.
            sqlx-cli
            sqlite
            # Conveniences.
            actionlint
            cargo-watch
            taplo
            typos
            cargo-machete
            # Branch, then pull request: the workflow needs a GitHub client, and
            # an agent that cannot open the PR leaves the work on a pushed
            # branch nobody has been asked to review.
            gh
            # agent
            claude-code
          ];
        };
      }
    );

}
