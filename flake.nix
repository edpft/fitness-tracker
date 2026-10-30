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

        # Every build uses the same file set: cargo needs the whole workspace
        # manifest graph, and sqlx
        # needs the migrations and the offline query metadata at *compile*
        # time. `cleanCargoSource` would drop both — it keeps only files cargo
        # itself recognises — so the fileset is spelled out here and used
        # everywhere.
        src = lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            # One entry per workspace member:
            (craneLib.fileset.commonCargoSources ./crates/domain)
            (craneLib.fileset.commonCargoSources ./crates/application)
            (craneLib.fileset.commonCargoSources ./crates/infrastructure)
            (craneLib.fileset.commonCargoSources ./crates/configurator)
            (craneLib.fileset.commonCargoSources ./crates/telemetry)
            (craneLib.fileset.commonCargoSources ./crates/web)
            (craneLib.fileset.commonCargoSources ./crates/cli)
            (craneLib.fileset.commonCargoSources ./crates/xtask)
            # htmx and its server-sent events extension, which `web` embeds
            # with `include_str!` and serves (§ 20).
            ./crates/web/assets
            # `sqlx::migrate!` embeds these, and `query!` is verified against
            # the schema they produce.
            ./migrations
            # `telemetry.db` has its own, which are not the store's (#631).
            ./crates/telemetry/migrations
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
            # nextest's JUnit report, which the `test` check keeps (#629).
            ./.config/nextest.toml
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

        # **What a build is.** There are no releases here: the operator
        # installs with `nix profile` from `main` and upgrades in place, so the
        # identity of a build is the commit it came from, not a number someone
        # cut. A number that moved only on release named the wrong code —
        # `--version` said `0.2.0` for every build installed after 2026-08-28.
        #
        # `shortRev` exists only for a clean tree; a dirty one carries
        # `dirtyShortRev`, and a source with no git has neither.
        revision = self.shortRev or self.dirtyShortRev or "unknown";

        # `lastModifiedDate` is `YYYYMMDDHHMMSS`. The date is the half an
        # operator recognises — "the build I installed last week".
        revisionDate =
          let
            stamp = self.lastModifiedDate or "00000000";
            part = start: length: builtins.substring start length stamp;
          in
          "${part 0 4}-${part 4 2}-${part 6 2}";

        # Deliberately *not* the revision. This sits in `commonArgs`, so it
        # names `cargoArtifacts` too — and a name that changed every commit
        # would rebuild every dependency every commit. It agrees with the
        # `0.0.0` the crates carry; `cli` overrides it with something useful.
        version = "0.0.0";

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
          ++ lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
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
        # `configurator` connects adapters to ports for both frontends — the
        # composition root, in Cockburn's sense — so it sits between them and
        # `infrastructure` (#398). `cli` and `web` are peers above it: two
        # driving adapters, neither depending on the other. Equal ring numbers
        # make that structural — the check requires a strict decrease across
        # every edge, so a dependency either way fails.
        crateRings = {
          domain = 0;
          application = 1;
          infrastructure = 2;
          # Beside the adapters, but any ring above may record spans with it,
          # and the tooling outside the rings will (#629). It depends on
          # nothing in the app.
          telemetry = 2;
          configurator = 3;
          web = 4;
          cli = 4;
        };

        # A strict decrease is not enough on its own: a frontend could still
        # reach past the configurator to an adapter, and compose for itself what
        # the configurator composes for both (#402). So a crate named here may
        # be depended on by the one crate it names, and by nothing else.
        crateOnlyDependent = {
          infrastructure = "configurator";
        };

        # Tooling, not the app, so in no ring (#629). A tooling crate may depend
        # on what is named here and on nothing else in the app, and nothing in
        # the app may depend on a tooling crate.
        toolingCrates = [ "xtask" ];
        toolingMayUse = [ "telemetry" ];

        # The workspace's own crates, compiled once for every check that needs
        # them (#645). Until then each crate had a check of its own, and each of
        # those, `test` and `doctest` compiled the workspace again from
        # `cargoArtifacts`: `domain` was built nine times a gate. What `test`
        # compiles is what this compiles, so `test`, `doctest` and the binaries
        # start from it and compile nothing of the workspace's again.
        #
        # One derivation over one `src`, so any change recompiles every crate
        # once. Rebuilding only what a change reaches is #654.
        #
        # `mkCargoDerivation` rather than `cargoBuild`, which always runs
        # `cargo build` and so built no test target.
        workspaceArtifacts = craneLib.mkCargoDerivation (
          commonArgs
          // {
            inherit cargoArtifacts;
            pname = "workspace-build";
            # As nextest builds before it runs anything.
            buildPhaseCargoCommand = "cargoWithProfile test --locked --workspace --no-run";
            doCheck = false;
          }
        );

        # Both binaries, from one build. `-p cli` and `-p web` resolved
        # features for that crate alone, which rebuilt about twenty-five
        # dependencies and every workspace crate beneath it, and `--bins`
        # leaves the dev-dependencies' features out, which rebuilt the crates
        # using sqlx. `--all-targets` resolves as the tests do, so the binaries
        # are the ones the tests ran against and only what reads
        # `FITNESS_BUILD` compiles again. Test executables are not installed.
        binaries = craneLib.buildPackage (
          individualCrateArgs
          // {
            cargoArtifacts = workspaceArtifacts;
            pname = "binaries";
            cargoExtraArgs = "--locked --workspace --all-targets";

            # What `fitness --version` prints, read by `option_env!` at compile
            # time, and for `web` which build derived the store's layers
            # (#468): a run the page starts is a build like any other, and one
            # that cannot name its commit derives everything. Set here rather
            # than in `commonArgs` on purpose: it changes with every commit, and
            # there it would invalidate `cargoArtifacts` and rebuild every
            # dependency each time.
            FITNESS_BUILD = "${revision} (${revisionDate})";
          }
        );

        # One binary from `binaries`, as a package of its own. Named for the
        # commit, so `nix profile list` says which build is installed.
        binary =
          name: program:
          pkgs.runCommand "${name}-${revision}" { meta.mainProgram = program; } ''
            mkdir -p "$out/bin"
            ln -s ${binaries}/bin/${program} "$out/bin/${program}"
          '';

        # The crate is `cli`; the binary operators type is `fitness`.
        cli = binary "cli" "fitness";
        web = binary "web" "web";

        # A user service, so the page is up whenever the laptop is without
        # anyone starting it from a terminal (#399). It names the store path
        # of `web` rather than putting `web` on PATH, and the reference keeps
        # that build alive for as long as the profile holds the unit.
        #
        # **No address here** (§ 34): `FITNESS_WEB_LISTEN` comes from a
        # drop-in on the machine, as the firewall rule admitting the phone
        # does. Without one `web` exits saying `--listen` is required, and
        # the restart delay keeps that from spinning.
        #
        # `always`, not `on-failure`: systemd counts SIGTERM as a clean exit,
        # so a stray `kill` left the page down until someone noticed (#430).
        # `systemctl --user stop` still stops it.
        webService = pkgs.writeTextFile {
          name = "fitness-web-service";
          destination = "/share/systemd/user/fitness-web.service";
          text = ''
            [Unit]
            Description=The fitness tracker's web interface

            [Service]
            ExecStart=${lib.getExe web}
            Restart=always
            RestartSec=10

            [Install]
            WantedBy=default.target
          '';
        };

        # `nix profile upgrade` runs no hook, and a running service keeps the
        # unit it was started from, so the page went on serving the build it
        # replaced (#405). This watches the profile instead: any change to it
        # reloads the user units and restarts the page if it is running. A
        # change that is not an upgrade of this flake costs a restart.
        #
        # The directory, not `profile` in it: an upgrade adds a generation
        # here and then renames a new `profile` symlink over the old one, and
        # a watch on the symlink sees no rename. The generation fires the
        # watch first, so the rename can land while the reload is running,
        # when the path unit drops events; the loop goes round until `profile`
        # stays put. `systemctl` is the host's, inherited from the service's
        # PATH, so it speaks the running manager's version.
        followProfile = pkgs.writeShellApplication {
          name = "fitness-web-follow-profile";
          runtimeInputs = [ pkgs.coreutils ];
          text = ''
            profile="$1"
            while true; do
              before="$(readlink "$profile")"
              systemctl --user daemon-reload
              systemctl --user try-restart fitness-web.service
              [ "$(readlink "$profile")" = "$before" ] && break
            done
          '';
        };

        webUpgradePath = pkgs.writeTextFile {
          name = "fitness-web-upgrade-path";
          destination = "/share/systemd/user/fitness-web-upgrade.path";
          text = ''
            [Unit]
            Description=Serve the newly installed build of the fitness tracker's web interface

            [Path]
            PathChanged=%h/.local/state/nix/profiles

            [Install]
            WantedBy=default.target
          '';
        };

        webUpgradeService = pkgs.writeTextFile {
          name = "fitness-web-upgrade-service";
          destination = "/share/systemd/user/fitness-web-upgrade.service";
          text = ''
            [Unit]
            Description=Serve the newly installed build of the fitness tracker's web interface

            [Service]
            Type=oneshot
            ExecStart=${lib.getExe followProfile} %h/.local/state/nix/profiles/profile
          '';
        };

        # A merge reached the laptop only when someone ran `nix profile
        # upgrade` (#502), so the laptop polls for it. It follows the
        # `published` tag CI moves once a build is in the cache, not `main`,
        # whose head may never be built. `--refresh` because `tarball-ttl` is
        # an hour, and the tag would be up to that stale.
        #
        # `--max-jobs 0`: it installs the build CI pushed to Cachix and never
        # makes its own: anything it would have to build is refused.
        #
        # **Only when `main` has moved.** An upgrade that finds nothing new
        # adds no generation but still rewrites `profile` onto the same one,
        # and `fitness-web-upgrade.path` restarted the page every tick for it.
        # So it compares the locked reference installed with the one the tag
        # resolves to now, and leaves the profile untouched when they agree.
        #
        # Two settings `--max-jobs 0` needs from a cache, which are the
        # daemon's and so live in the host's `/etc/nix/nix.conf`: passed here
        # by a user it does not trust, it ignores them. Nixpkgs marks its
        # trivial builders (the units here, the `symlinkJoin` holding them) as
        # never worth substituting, so without `always-allow-substitutes =
        # true` they are refused rather than fetched. And nix remembers a
        # cache's "not here" for an hour, so without
        # `narinfo-cache-negative-ttl = 0` a tick that looked before CI's push
        # landed would wait that long after it.
        #
        # `nix` is the host's, from the user manager's PATH, as `systemctl` is
        # above: a shell resolves it there, where systemd resolving a bare
        # `ExecStart` would search only its own compiled-in directories.
        upgrade = pkgs.writeShellApplication {
          name = "fitness-upgrade";
          runtimeInputs = [ pkgs.jq ];
          text = ''
            element=fitness-tracker
            installed="$(nix profile list --json | jq -er --arg e "$element" '.elements[$e]')"
            latest="$(nix flake metadata --refresh --json "$(jq -er .originalUrl <<<"$installed")" | jq -er .url)"
            [ "$(jq -er .url <<<"$installed")" = "$latest" ] && exit 0
            exec nix profile upgrade --refresh --max-jobs 0 "$element"
          '';
        };

        upgradeService = pkgs.writeTextFile {
          name = "fitness-upgrade-service";
          destination = "/share/systemd/user/fitness-upgrade.service";
          text = ''
            [Unit]
            Description=Install the fitness tracker's latest build from main

            [Service]
            Type=oneshot
            ExecStart=${lib.getExe upgrade}
          '';
        };

        upgradeTimer = pkgs.writeTextFile {
          name = "fitness-upgrade-timer";
          destination = "/share/systemd/user/fitness-upgrade.timer";
          text = ''
            [Unit]
            Description=Install the fitness tracker's latest build from main

            [Timer]
            OnCalendar=*:0/5

            [Install]
            WantedBy=timers.target
          '';
        };

        # What `nix profile` installs: the `fitness` command and the units that
        # serve the page and keep it current, upgraded together.
        installed = pkgs.symlinkJoin {
          name = "fitness-tracker-${revision}";
          paths = [
            cli
            webService
            webUpgradePath
            webUpgradeService
            upgradeService
            upgradeTimer
          ];
          meta.mainProgram = "fitness";
        };

      in
      {
        checks = {
          inherit web cli;

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
              cargoArtifacts = workspaceArtifacts;

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

              # nextest's report, so `cargo xtask` can record each test (#629).
              # A run that fails keeps nothing: nix keeps no output of a failed
              # build, so only a passing suite's tests are recorded from here.
              postCheck = ''
                cp "''${CARGO_TARGET_DIR:-target}/nextest/default/junit.xml" "$out/junit.xml"
              '';
            }
          );

          # nextest does not run doctests, so they need their own check.
          doctest = craneLib.cargoDocTest (
            commonArgs
            // {
              cargoArtifacts = workspaceArtifacts;
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

                printf '%s\n' ${
                  lib.escapeShellArgs (
                    lib.mapAttrsToList (crate: dependent: "${crate} ${dependent}") crateOnlyDependent
                  )
                } > only

                printf '%s\n' ${lib.escapeShellArgs toolingCrates} > tooling
                printf '%s\n' ${lib.escapeShellArgs toolingMayUse} > usable

                awk '
                  FILENAME == "rings" { ring[$1] = $2; next }
                  FILENAME == "only" { only[$1] = $2; next }
                  FILENAME == "tooling" { tooling[$1] = 1; next }
                  FILENAME == "usable" { usable[$1] = 1; next }
                  ($1 in tooling) && !($2 in tooling) && !($2 in usable) {
                    print "  " $1 " is tooling, and depends on " $2; bad = 1; next
                  }
                  ($2 in tooling) && !($1 in tooling) {
                    print "  " $1 " depends on " $2 ", which is tooling"; bad = 1; next
                  }
                  ($1 in tooling) { next }
                  !($1 in ring) { print "  " $1 " is in no ring"; bad = 1; next }
                  !($2 in ring) { print "  " $2 " is in no ring"; bad = 1; next }
                  ring[$2] >= ring[$1] {
                    printf "  %s (ring %d) depends on %s (ring %d)\n", $1, ring[$1], $2, ring[$2]
                    bad = 1
                  }
                  ($2 in only) && $1 != only[$2] {
                    printf "  %s depends on %s, which only %s may\n", $1, $2, only[$2]
                    bad = 1
                  }
                  END { exit bad }
                ' rings only tooling usable edges && exit_code=0 || exit_code=1

                if [ "$exit_code" -ne 0 ]; then
                  echo
                  echo "Constitution § 16: dependencies point inward only —"
                  echo "{cli, web} -> configurator -> infrastructure -> application -> domain. A crate"
                  echo "in no ring is a new one: give it a ring in flake.nix,"
                  echo "which is the decision worth making deliberately. A frontend"
                  echo "gets its adapters from the configurator, never directly."
                  echo "Tooling sits outside the rings and uses telemetry alone."
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
          # `application` keeps its use cases behind their modules and
          # re-exports only its ports and errors at the crate root, so naming a
          # use case requires naming its module. That makes this greppable,
          # which is the only reason the module boundary is worth keeping.
          #
          # **What infrastructure may name, not what it may not.** This listed
          # the forbidden modules until 2026-10-30, and the list went stale
          # twice: `prescribe` was unguarded from feature 003 until 2026-08-19,
          # and by #644 it named seven of thirty. So any module beneath
          # `application` other than `ports` and `error` fails, inside a `use`
          # group as well as in a path. The crate root's re-exports are all
          # types, so a lowercase name there is a module.
          #
          # Scoped to `src`, not the whole crate. An integration test at this ring
          # *does* drive a use case, and legitimately: the normalisation and
          # prescription suites need the real translator and the real store, and
          # `application` may not depend on the ring above it. So the rule is
          # about the adapter, and `tests/` is not the adapter.
          #
          # **`web` was checked here until 2026-08-30 and should not have been.**
          # Every word above is about a *driven* adapter reaching past the ports
          # it implements. `web` is a *driving* adapter — an operator entry point
          # and a composition root, at ring 3 beside `cli` — and calling use
          # cases is the whole of its job, as it is `cli`'s, which was never
          # checked. The mistake was invisible only because `web` was a stub that
          # called nothing; the first real line of it would have failed the gate.
          use-case-isolation =
            pkgs.runCommand "use-case-isolation"
              {
                nativeBuildInputs = [ pkgs.perl ];
              }
              ''
                find ${repoSrc}/crates/infrastructure/src -name '*.rs' -print0 \
                  | xargs -0 perl -0777 -ne '
                      s{//[^\n]*}{}g;
                      while (/\bapplication::(\{(?:[^{}]|\{[^{}]*\})*\}|[A-Za-z_]\w*)/g) {
                        my $named = $1;
                        $named =~ s/\{[^{}]*\}//g while $named =~ /^\{.*\{/s;
                        for my $segment ($named =~ /(?:^|[{,])\s*([a-z_]\w*)/g) {
                          print "  $ARGV names application::$segment\n"
                            unless $segment eq "ports" || $segment eq "error";
                        }
                      }
                    ' > named

                if [ -s named ]; then
                  cat named
                  echo
                  echo "Constitution § 16: a driven adapter implements ports, it"
                  echo "does not call the use cases. What infrastructure may name"
                  echo "from application is its ports and its errors."
                  echo
                  echo "Driving adapters — cli, web — are not checked here: calling"
                  echo "a use case is what they are for."
                  exit 1
                fi

                touch "$out"
              '';

          # Not constitutional — build hygiene. `src` lists its members
          # by hand, so a new crate can be perfectly valid to cargo while nix
          # silently ignores its sources, and every build keeps passing
          # without ever compiling it.
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
                  echo "flake.nix, so nix is not building it. Add it there."
                  exit 1
                fi

                touch "$out"
              '';

          # Not constitutional — build time. Cargo makes each file in `tests/`
          # its own crate, optimised and linked against every dependency, and
          # 111 of them were most of the `test` check's 26 minutes of
          # compiling (#648). So a crate has one test target, `tests/main.rs`,
          # and the files beside it are its modules. With cargo's discovery
          # off, a file `main.rs` does not name is never compiled, and its
          # tests pass by not running: that is what this catches, along with a
          # second test target.
          test-targets =
            pkgs.runCommand "test-targets"
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
                  | jq -r '.packages[] | select([.targets[] | select(.kind == ["test"])] | length > 1)
                      | "  \(.name) has more than one test target"' > found

                for tests in ${repoSrc}/crates/*/tests; do
                  crate="$(basename "$(dirname "$tests")")"
                  for entry in "$tests"/*.rs "$tests"/*/mod.rs; do
                    [ -e "$entry" ] || continue
                    case "$entry" in
                      */mod.rs) name="$(basename "$(dirname "$entry")")" ;;
                      *) name="$(basename "$entry" .rs)" ;;
                    esac
                    [ "$name" = main ] && continue
                    grep -qx "mod $name;" "$tests/main.rs" 2>/dev/null \
                      || echo "  $crate/tests/main.rs does not name $name" >> found
                  done
                done

                if [ -s found ]; then
                  cat found
                  echo
                  echo "A crate's integration tests are one binary, tests/main.rs,"
                  echo "and each file beside it is a module it names (#648). Add"
                  echo "the module to main.rs rather than another test target."
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
              # **From inside the tree, not with the tree as an argument.**
              # typos reads `_typos.toml`'s exclusions relative to where it
              # runs, so pointed at the source from the build directory it
              # excluded nothing — invisible until vendored htmx arrived (#398).
              ''
                cd ${
                  lib.fileset.toSource {
                    root = ./.;
                    fileset = ./.;
                  }
                }
                typos .
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

          # cargo-deny rather than cargo-audit because it reports an ignore that
          # no longer matches (#492), and denying that makes an ignore expire
          # with its cause. It reads the database from a git checkout under
          # CARGO_HOME, named for the default URL, and dates it by HEAD.
          audit-deps = craneLib.cargoDeny {
            inherit src version;
            pname = "workspace-advisories";
            cargoDenyExtraArgs = "--offline";
            cargoDenyChecks = "--deny advisory-not-detected advisories";
            nativeBuildInputs = [ pkgs.git ];
            preBuild = ''
              db="$CARGO_HOME/advisory-dbs/advisory-db-3157b0e258782691"
              mkdir -p "$db"
              cp -r ${advisory-db}/. "$db"
              chmod -R u+w "$db"
              export GIT_AUTHOR_DATE="@${toString advisory-db.lastModified} +0000"
              export GIT_COMMITTER_DATE="$GIT_AUTHOR_DATE"
              git -C "$db" init -q
              git -C "$db" add -A
              git -C "$db" -c user.name=nix -c user.email=nix@localhost commit -qm advisory-db
            '';
          };

          audit-licenses = craneLib.cargoDeny {
            inherit src version;
            pname = "workspace";
          };
        };

        packages = {
          # The default is what the operator installs, so `nix run` hands you
          # `fitness` and `nix profile upgrade` brings the web service with it.
          # Both binaries are exposed by name.
          default = installed;
          inherit cli web;

          # Named for CI rather than for anyone to install: each check runs on
          # its own runner, and `prebuild` builds these once for all of them
          # instead of every job building every dependency and every crate.
          # The dependencies go in the cache, and the workspace build, which
          # changes with every commit, is handed on as an artifact (#645).
          inherit cargoArtifacts workspaceArtifacts;
        };

        # Takes the files to format: `nix fmt .`, not a bare `nix fmt`.
        formatter = pkgs.nixfmt;

        devShells.default = craneLib.devShell {
          checks = self.checks.${system};
          packages = with pkgs; [
            # The tools CI uses, so a failure can be reproduced locally.
            cargo-deny
            cargo-nextest
            gitleaks
            # What `cargo xtask` runs the checks with (#629).
            nix-fast-build
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
