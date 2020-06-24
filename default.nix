rec {
  target = {
    # Output Artifacts
    releases = rec {
      # See `rustup target list`
      x86_64-unknown-linux-gnu = rec {
        configuration = {
          buildType = "debug";
          rustTarget = "x86_64-unknown-linux-gnu";
          hostPkgs = pkgs;
          targetPkgs = if pkgs.targetPlatform.config == pkgs.pkgsCross.gnu64.targetPlatform.config then
              pkgs
            else
              pkgs.pkgsCross.gnu64;
          logLevel = "trace";
          runCheckPhase = false;
          features = features.components.all ++
            features.byOs.linux.gnu ++
            features.byLinking.dynamic;
        };
        binary = tasks.binary configuration;
        docker = tasks.docker { tag = "gnu"; binary = binary; };
        all = [ binary docker ];
      };
      x86_64-unknown-linux-musl = rec {
        configuration = {
          buildType = "debug";
          rustTarget = "x86_64-unknown-linux-musl";
          hostPkgs = pkgs;
          targetPkgs = if pkgs.targetPlatform.config == pkgs.pkgsCross.musl64.targetPlatform.config then
              pkgs
            else
              pkgs.pkgsCross.musl64;
          logLevel = "debug";
          runCheckPhase = false;
          features = features.components.all ++
            features.byOs.linux.musl;
        };
        binary = tasks.binary configuration;
        docker = tasks.docker { tag = "musl"; binary = binary; };
        all = [ binary docker ];
      };
      all = [ x86_64-unknown-linux-gnu x86_64-unknown-linux-musl ];
    };
  };

  environment = {
    variables =  targetPkgs: {
      PKG_CONFIG_ALLOW_CROSS=true;
      # We must set some protoc related env vars for the prost crate.
      PROTOC = "${targetPkgs.protobuf}/bin/protoc";
      PROTOC_INCLUDE = "${targetPkgs.protobuf}/include";
      # On Linux builds, we need some level of localization.
      LOCALE_ARCHIVE = if targetPkgs.stdenv.isLinux && targetPkgs.glibcLocales != null then
        "${targetPkgs.glibcLocales}/lib/locale/locale-archive"
      else
        "";
      LC_ALL = "en_US.UTF-8";
      # Without setting a tzdata folder, some tests will fail.
      TZDIR = "${targetPkgs.tzdata}/share/zoneinfo";
      # Crates expect information about OpenSSL in these vars.
      OPENSSL_DIR = "${targetPkgs.openssl.dev}";
      OPENSSL_LIB_DIR = "${targetPkgs.openssl.out}/lib";
      SSL_CERT_FILE = "${targetPkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
      # Git looks to this env var for SSL certificates.
      GIT_SSL_CAINFO = "${targetPkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
      # Curl looks to this env var for SSL certificates.
      CURL_CA_BUNDLE = "${targetPkgs.cacert}/etc/ca-bundle.crt";
      # Encourage Cargo to be pretty.
      CARGO_TERM_COLOR = "always";
      # Enable backtraces in the environment.
      RUST_BACKTRACE = "full";
      # Vector gets very angry if you don't set these and use the AWS components.
      AWS_ACCESS_KEY_ID = "dummy";
      AWS_SECRET_ACCESS_KEY = "dummy";
      # Lucet (for wasm) depends on libclang
      # LIBCLANG_PATH="${targetPkgs.llvmPackages.libclang}/lib";
      CPATH= if targetPkgs.stdenv.isLinux then
        "${targetPkgs.linuxHeaders}/include"
      else
        "";
    };
    developmentTools = targetPkgs:
      with targetPkgs;
      [
        file
        dnsutils
        curl
        bash
        nix
        direnv
        remarshal
        libiconv
        tzdata
        jq
        stdenv
        bashInteractive
        rustup
        # Build Env
        git
        cacert
        ruby_2_7
        nodejs
        yarn
        shellcheck
        # Container tools
        docker
        docker-compose
        # Wasm
        llvmPackages.libclang
      ]  ++ (if stdenv.isDarwin then [
        # Mac only
      ] else [
        # Linux
        podman
        podman-compose
      ]);
    dependencies = {
      # From: https://nixos.org/nixpkgs/manual/
      #
      # A list of dependencies whose host platform is the new derivation's build platform, and target
      # platform is the new derivation's host platform. This means a -1 host offset and 0 target
      # offset from the new derivation's platforms. These are programs and libraries used at build-time
      # that, if they are a compiler or similar tool, produce code to run at run-time—i.e. tools used
      # to build the new derivation. If the dependency doesn't care about the target platform (i.e.
      # isn't a compiler or similar tool), put it here, rather than in depsBuildBuild or
      # depsBuildTarget. This could be called depsBuildHost but nativeBuildInputs is used for
      # historical continuity.
      #
      # Since these packages are able to be run at build-time, they are added to the PATH, as described
      # above. But since these packages are only guaranteed to be able to run then, they shouldn't
      # persist as run-time dependencies. This isn't currently enforced, but could be in the future.
      nativeBuildInputs = targetPkgs:
        with targetPkgs;
          [
            pkg-config
            openssl.dev
            jemalloc
            snappy
            rdkafka
          ]
          ++ (
            if stdenv.isDarwin then
              [
                darwin.cf-private
                darwin.apple_sdk.frameworks.CoreServices
                darwin.apple_sdk.frameworks.Security
                darwin.apple_sdk.frameworks.SecurityFoundation
              ]
            else if stdenv.isLinux then
              [
                # linuxHeaders
              ]
            else
              []
          ); 

      # From: https://nixos.org/nixpkgs/manual/
      #
      # A list of dependencies whose host platform and target platform match the new derivation's.
      # This means a 0 host offset and a 1 target offset from the new derivation's host platform. This
      # would be called depsHostTarget but for historical continuity. If the dependency doesn't care
      # about the target platform (i.e. isn't a compiler or similar tool), put it here, rather than in
      # depsBuildBuild.
      #
      # These are often programs and libraries used by the new derivation at run-time, but that isn't
      # always the case. For example, the machine code in a statically-linked library is only used at
      # run-time, but the derivation containing the library is only needed at build-time. Even in the
      # dynamic case, the library may also be needed at build-time to appease the linker.
      buildInputs = targetPkgs:
        with targetPkgs;
        [
          # pkg-config
          # protobuf
          # rustup
          # rdkafka
          # cmake
          # openssl.dev
        ] ++ (
          if stdenv.isDarwin then
            []
          else if stdenv.isLinux then
            [
              # systemd
              # gcc
            ]
          else
            []
        );
    };
  };

  ### Development code.

  cargoToml = (builtins.fromTOML (builtins.readFile ./Cargo.toml));

  overlays = rec {
    # mozilla = import (builtins.fetchTarball "https://github.com/matthewbauer/nixpkgs-mozilla/archive/91629591f14262e4d6059c5407d065407b12c5d6.tar.gz");
    mozilla = import (builtins.fetchGit {
        url = "https://github.com/mozilla/nixpkgs-mozilla/";
        rev = "e912ed483e980dfb4666ae0ed17845c4220e5e7c";
      });
  };

  pkgs = import <nixpkgs> {
    overlays = [
      overlays.mozilla
    ];
  };

  # Handy feature aliases for use in `configurations`
  features = {
    components = rec {
      sources = cargoToml.features.sources;
      sinks = cargoToml.features.sinks;
      transforms = cargoToml.features.transforms;
      all = sources ++ sinks ++ transforms;
    };
    byLinking = {
      static = ["rdkafka"];
      dynamic = ["rdkafka" "rdkafka/dynamic_linking"];
    };
    byOs = {
      linux = {
        # Linux is *special* and has two of differing characteristics.
        gnu = [ "unix" "leveldb" ];
        musl = [ ];
      };
      mac = ["unix"];
      windows = [];
      freebsd = [];
    };
  };

  
  # Jobs used to build artifacts
  tasks = rec {
    docker = args@{ binary, tag }:
      pkgs.dockerTools.buildImage {
        name = "neu-timberio/vector";
        tag = args.tag;
        config.Cmd = [ "${args.binary.out}/bin/vector" ];
      };
    # Build a binary Vector artifact
    binary = args@{
      # This will be set dynamically!
      features ? null,
      linking ? "dynamic",
      rustChannel ? null, # Defaulted below
      rustTarget,
      hostPkgs,
      targetPkgs,
      buildType ? "debug",
      logLevel ? "debug",
      runCheckPhase ? true,
    }:
      let
        rustChannel = hostPkgs.rustChannelOf {
          rustToolchain = ./rust-toolchain;
          # DO NOT TRY TO PUT THIS HERE. PUT IT IN THE `(this).rust.override { ... }`
          # targets = [ args.rustTarget ];
        };
        features = (builtins.getAttr args.rustTarget target.releases).configuration.features;
        
        packageDefinition = rec {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;
          # See `definition.nix` for details on these.
          nativeBuildInputs = (environment.dependencies.nativeBuildInputs targetPkgs);
          buildInputs = (environment.dependencies.buildInputs hostPkgs);
          passthru = (environment.variables targetPkgs);
          # Configurables
          buildType = args.buildType;
          logLevel = args.logLevel;
          cargoSha256 = "0yld7dczz27i5bzi39pr1fxxxfmdaqxl80hymxbqxdnwhhq0hjch";
          
          target = args.rustTarget;
          # Rest
          src = hostPkgs.lib.cleanSource (tools.gitignore.gitignoreSource ./.);

          cargoBuildFlags = [ "--verbose" "--no-default-features" "--features" "${hostPkgs.lib.concatStringsSep "," features}" ];
          checkPhase = if runCheckPhase then
              ''
              # Configurables
              export TZDIR=${targetPkgs.tzdata}/share/zoneinfo
              cargo test --no-default-features --features ${hostPkgs.lib.concatStringsSep "," features} -- --test-threads 1
              ''
            else
              "";
          
          # cargoBuildOptions = currentOptions: currentOptions ++ [ "--no-default-features" "--features" "${pkgs.lib.concatStringsSep "," features}" ];
          # cargoTestOptions = currentOptions: currentOptions ++ [ "--no-default-features" "--features" "${pkgs.lib.concatStringsSep "," features}" ];
          # cargoTestCommands = currentOptions: if runCheckPhase then
          #   currentOptions
          # else
          #   [];
          meta = with targetPkgs.stdenv.lib; {
            description = "A high-performance logs, metrics, and events router";
            homepage    = "https://github.com/timberio/vector";
            license = licenses.asl20;
            maintainers = [];
            platforms = platforms.all;
          };
        } // (environment.variables targetPkgs);
      in
        # (tools.naersk.buildPackage packageDefinition);
        (targetPkgs.makeRustPlatform {
          cargo = rustChannel.rust.override {
            targets = [ args.rustTarget ];
          };
          rustc = rustChannel.rust.override {
            targets = [ args.rustTarget ];
          };
        }).buildRustPackage packageDefinition;
  };

  tools = {
      naersk = pkgs.callPackage (import (builtins.fetchGit {
        url = "https://github.com/nmattia/naersk/";
        rev = "a82fd7dc31a58c462b6dfa9d9d886fa2cc75dfd4";
      })) {};
      # This tool lets us ignore things in our `.gitignore` during a nix build. Very Handy.
      gitignore = import (builtins.fetchGit {
        url = "https://github.com/hercules-ci/gitignore/";
        rev = "647d0821b590ee96056f4593640534542d8700e5";
      }) { inherit (pkgs) lib; };
      # cargo2nix = import (builtins.fetchGit {
      #   url = "https://github.com/tenx-tech/cargo2nix/";
      #   rev = "f6d835482fbced7a9c2aa4fa270a179ed4f9c0f3";
      # }) {};
  };
}