{
  description = "Vector — observability data pipeline (Nix build, PoC).";

  # Only Linux systems are supported as *builders* right now; darwin users
  # need a remote Linux builder to produce the musl artifact.
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, crane, rust-overlay, flake-utils }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        inherit (pkgs) lib;

        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        vectorVersion = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;

        # build.rs shells out to `git rev-parse --short HEAD`. The Nix source
        # filter drops `.git`, so we shim `git` with a wrapper that returns
        # the flake revision (or a placeholder for dirty trees).
        gitHash =
          if self ? rev then builtins.substring 0 9 self.rev
          else if self ? dirtyRev then builtins.substring 0 9 self.dirtyRev
          else "nixbuild";

        gitShim = pkgs.writeShellScriptBin "git" ''
          if [ "$1" = "rev-parse" ] && [ "$2" = "--short" ] && [ "$3" = "HEAD" ]; then
            printf '%s' "${gitHash}"
            exit 0
          fi
          exec ${pkgs.git}/bin/git "$@"
        '';

        src = lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            let bn = baseNameOf (toString path); in
            !(bn == "target" || bn == "result" || bn == ".git"
              || bn == "node_modules" || bn == ".direnv");
        };

        # Rust target triple -> nixpkgs autoconf config. Mostly identical,
        # except ARM: rust uses `armv7`/`arm`, nixpkgs uses `armv7l`/`arm`.
        nixpkgsConfigForRustTarget = target: {
          "x86_64-unknown-linux-gnu"       = "x86_64-unknown-linux-gnu";
          "x86_64-unknown-linux-musl"      = "x86_64-unknown-linux-musl";
          "aarch64-unknown-linux-gnu"      = "aarch64-unknown-linux-gnu";
          "aarch64-unknown-linux-musl"     = "aarch64-unknown-linux-musl";
          "armv7-unknown-linux-gnueabihf"  = "armv7l-unknown-linux-gnueabihf";
          "armv7-unknown-linux-musleabihf" = "armv7l-unknown-linux-musleabihf";
          # rust's `arm-*` targets default to cpu=arm1176jzf-s (ARMv6);
          # armv5tel works for gnueabi but musleabi's libatomic on v5 has
          # unresolved __sync_synchronize (no hw memory barrier, no kernel
          # helper fallback). ARMv6 has DMB natively.
          "arm-unknown-linux-gnueabi"      = "armv5tel-unknown-linux-gnueabi";
          "arm-unknown-linux-musleabi"     = "armv6l-unknown-linux-musleabi";
        }.${target};

        crossPkgsForTarget = target:
          import nixpkgs {
            inherit system;
            crossSystem = { config = nixpkgsConfigForRustTarget target; };
            overlays = [ (import rust-overlay) ];
          };

        targets = [
          "x86_64-unknown-linux-gnu"
          "x86_64-unknown-linux-musl"
          "aarch64-unknown-linux-gnu"
          "aarch64-unknown-linux-musl"
          "armv7-unknown-linux-gnueabihf"
          "armv7-unknown-linux-musleabihf"
          "arm-unknown-linux-gnueabi"
          "arm-unknown-linux-musleabi"
        ];

        mkVector = { target, cargoFeatures }:
          let
            rustWithTarget = rustToolchain.override { targets = [ target ]; };
            craneLib = (crane.mkLib pkgs).overrideToolchain rustWithTarget;

            # Some upstream forks (e.g. vector's patched tracing) declare
            # `readme = "..."` in workspace subcrate manifests without shipping
            # the file, which breaks crane's `cargo package` vendoring. Create
            # empty placeholders in each git-dep checkout before packaging.
            cargoVendorDir = craneLib.vendorCargoDeps {
              inherit src;
              overrideVendorGitCheckout = _pkgs: drv:
                drv.overrideAttrs (old: {
                  postPatch = (old.postPatch or "") + ''
                    find . -name Cargo.toml | while read -r f; do
                      dir=$(dirname "$f")
                      if grep -qE '^[[:space:]]*readme[[:space:]]*=' "$f" \
                         && [ ! -f "$dir/README.md" ]; then
                        touch "$dir/README.md"
                      fi
                    done
                  '';
                });
              # sasl2-sys 0.1.22+2.1.28 build.rs does `cp -R sasl2 $OUT_DIR/sasl2`
              # but the copy lands read-only in a Nix sandbox, breaking
              # autoconf's config.log write. Patch the build.rs to chmod +w
              # after the copy. No matching upstream issue yet; closest is
              # MaterializeInc/rust-sasl#54 (cross-compile toolchain detection).
              overrideVendorCargoPackage = package: drv:
                if package.name == "sasl2-sys" then
                  drv.overrideAttrs (old: {
                    postPatch = (old.postPatch or "") + ''
                      substituteInPlace build.rs \
                        --replace-fail \
                          'cmd!("cp", "-R", "sasl2", &src_dir)' \
                          'cmd!("cp", "-R", "sasl2", &src_dir).run().expect("cp failed"); cmd!("chmod", "-R", "u+w", &src_dir)'
                    '';
                  })
                else drv;
            };

            crossPkgs = crossPkgsForTarget target;
            targetCc = crossPkgs.stdenv.cc;
            targetPrefix = targetCc.targetPrefix;

            # Rust/cc-rs env vars: dashes → underscores in the target triple.
            targetUnderscore = builtins.replaceStrings [ "-" ] [ "_" ] target;
            targetEnv = lib.toUpper targetUnderscore;
          in
          craneLib.buildPackage ({
            inherit src cargoVendorDir;
            pname = "vector";
            version = "0.0.0-nix-${gitHash}";

            strictDeps = true;
            doCheck = false;

            # Stop fortify-source macros from leaking into musl C builds;
            # musl doesn't export __memcpy_chk / __snprintf_chk / etc.
            hardeningDisable = [ "fortify" "fortify3" ];

            CARGO_BUILD_TARGET = target;
            CARGO_INCREMENTAL = "0";

            # .cargo/config.toml points musl targets at /lib/native-libs,
            # a path that only exists inside cross-rs's docker image. Strip
            # it for Nix builds; rust's self-contained musl libs cover the link.
            postPatch = ''
              substituteInPlace .cargo/config.toml \
                --replace-quiet '"-Lnative=/lib/native-libs"' '""'
            '';

            cargoExtraArgs = "--no-default-features --features ${cargoFeatures} --bin vector";

            nativeBuildInputs = [
              gitShim
              targetCc
              pkgs.protobuf
              pkgs.cmake
              pkgs.perl
              pkgs.pkg-config
              pkgs.rustPlatform.bindgenHook
            ];
          } // {
            # Target-scoped env vars steer cc-rs + rustc at the musl toolchain
            # for C deps and the final link. Host-side tools (build.rs, protoc)
            # continue to use the default glibc stdenv.
            "CC_${targetUnderscore}" = "${targetCc}/bin/${targetPrefix}cc";
            "CXX_${targetUnderscore}" = "${targetCc}/bin/${targetPrefix}c++";
            "AR_${targetUnderscore}" = "${targetCc.bintools}/bin/${targetPrefix}ar";
            "CARGO_TARGET_${targetEnv}_LINKER" = "${targetCc}/bin/${targetPrefix}cc";
          } // lib.optionalAttrs (target == "x86_64-unknown-linux-gnu") {
            # krb5-src (vendored by rdkafka?/gssapi-vendored, only on this
            # target) uses K&R function definitions that GCC 15's C23 default
            # rejects. Pin C dialect just for this target to avoid busting
            # the cache for the other 7.
            CFLAGS = "-std=gnu17";
          });

        vectorBins = lib.listToAttrs (map (t: {
          name = "vector-${t}";
          value = mkVector { target = t; cargoFeatures = "target-${t}"; };
        }) targets);

        # Tarball must match `scripts/package-archive.sh` output byte-shape so
        # distribution/docker/*/Dockerfile and downstream packaging consume it
        # unchanged. Notable quirks: LICENSE is LICENSE+NOTICE concatenated;
        # etc/systemd contains only vector.service; entries prefixed with `./`
        # so the Dockerfile's `--strip-components=2` lands correctly.
        mkTarball = { target, bin }:
          pkgs.runCommand "vector-${vectorVersion}-${target}.tar.gz" {
            nativeBuildInputs = [ pkgs.gnutar pkgs.gzip ];
          } ''
            root="./vector-${target}"
            mkdir -p "$root/bin" "$root/etc/systemd" "$root/etc/init.d" "$root/licenses"

            cp ${bin}/bin/vector "$root/bin/vector"

            cp ${./README.md} "$root/README.md"
            cat ${./LICENSE} ${./NOTICE} > "$root/LICENSE"
            cp ${./LICENSE-3rdparty.csv} "$root/LICENSE-3rdparty.csv"
            cp -R ${./licenses}/. "$root/licenses/"

            cp -R ${./config}/. "$root/config/"

            cp ${./distribution/systemd/vector.service} "$root/etc/systemd/vector.service"
            cp ${./distribution/init.d/vector} "$root/etc/init.d/vector"

            tar cf - "$root" | gzip -9 > "$out"
          '';

        tarballs = lib.listToAttrs (map (t: {
          name = "vector-tarball-${t}";
          value = mkTarball { target = t; bin = vectorBins."vector-${t}"; };
        }) targets);
      in
      {
        packages = vectorBins // tarballs // {
          default = vectorBins.vector-x86_64-unknown-linux-musl;
        };
      });
}
