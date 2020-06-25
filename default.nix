rec {
  target = {
    # Output Artifacts
    artifacts = rec {
      # See `rustup target list`
      x86_64-unknown-linux-gnu = rec {
        configuration = {
          setInterpreterPath = "/lib64/ld-linux-x86-64.so.2";
          buildType = "debug";
          rustTarget = "x86_64-unknown-linux-gnu";
          hostPkgs = pkgs;
          targetPkgs = if pkgs.targetPlatform.config == pkgs.pkgsCross.gnu64.stdenv.targetPlatform.config then
              pkgs
            else
              pkgs.pkgsCross.gnu64;
          logLevel = "trace";
          runCheckPhase = false;
          features = features.components.all ++
            features.byOs.linux.gnu ++
            features.byLinking.static;
        };
        binary = tasks.binary configuration;
        docker = tasks.docker { tag = configuration.rustTarget; binary = binary; };
      };
      x86_64-unknown-linux-musl = rec {
        configuration = {
          setInterpreterPath = null;
          buildType = "debug";
          rustTarget = "x86_64-unknown-linux-musl";
          hostPkgs = pkgs;
          targetPkgs = if pkgs.targetPlatform.config == pkgs.pkgsCross.gnu64.stdenv.targetPlatform.config then
              pkgs.pkgsStatic  # Yes, this is musl!
            else
              pkgs.pkgsCross.musl64;
          logLevel = "debug";
          runCheckPhase = false;
          features = features.components.all ++
            features.byOs.linux.musl ++
            features.byLinking.static;
        };
        binary = tasks.binary configuration;
        docker = tasks.docker { tag = configuration.rustTarget; binary = binary; };
      };
      aarch64-unknown-linux-gnu = rec {
        configuration = {
          setInterpreterPath = "/lib64/ld-linux-aarch64.so.2";
          buildType = "debug";
          rustTarget = "aarch64-unknown-linux-gnu";
          hostPkgs = pkgs;
          targetPkgs = if pkgs.targetPlatform.config == pkgs.pkgsCross.aarch64-multiplatform.stdenv.targetPlatform.config then
              pkgs
            else
              pkgs.pkgsCross.aarch64-multiplatform;
          logLevel = "debug";
          runCheckPhase = false;
          features = features.components.all ++
            features.byOs.linux.gnu ++
            features.byLinking.static;
        };
        binary = tasks.binary configuration;
        docker = tasks.docker { tag = configuration.rustTarget; binary = binary; };
      };
      aarch64-unknown-linux-musl = rec {
        configuration = {
          setInterpreterPath = null;
          buildType = "debug";
          rustTarget = "aarch64-unknown-linux-musl";
          hostPkgs = pkgs;
          targetPkgs = if pkgs.targetPlatform.config == pkgs.pkgsCross.aarch64-multiplatform-musl.stdenv.targetPlatform.config then
              pkgs
            else
              pkgs.pkgsCross.aarch64-multiplatform-musl;
          logLevel = "debug";
          runCheckPhase = false;
          features = features.components.all ++
            features.byOs.linux.musl ++
            features.byLinking.static;
        };
        binary = tasks.binary configuration;
        docker = tasks.docker { tag = configuration.rustTarget; binary = binary; };
      };
      armv7-unknown-linux-gnueabihf = rec {
        configuration = {
          setInterpreterPath = "/lib64/ld-linux-armv7.so.2";
          buildType = "debug";
          rustTarget = "armv7-unknown-linux-gnueabihf";
          hostPkgs = pkgs;
          targetPkgs = if pkgs.targetPlatform.config == pkgs.pkgsCross.armv7l-hf-multiplatform.stdenv.targetPlatform.config then
              pkgs
            else
              pkgs.pkgsCross.armv7l-hf-multiplatform;
          logLevel = "debug";
          runCheckPhase = false;
          features = features.components.all ++
            features.byOs.linux.musl ++
            features.byLinking.static;
        };
        binary = tasks.binary configuration;
        docker = tasks.docker { tag = configuration.rustTarget; binary = binary; };
      };
      armv7-unknown-linux-musleabihf = rec {
        setInterpreterPath = null;
        configuration = {
          buildType = "debug";
          rustTarget = "armv7-unknown-linux-musleabihf";
          hostPkgs = pkgs;
          targetPkgs = if pkgs.targetPlatform.config == pkgs.pkgsCross.armv7l-hf-multiplatform.stdenv.targetPlatform.config then
              pkgs.pkgsStatic
            else
              pkgs.pkgsCross.armv7l-hf-multiplatform.pkgsStatic;
          logLevel = "debug";
          runCheckPhase = false;
          features = features.components.all ++
            features.byOs.linux.musl ++
            features.byLinking.static;
        };
        binary = tasks.binary configuration;
        docker = tasks.docker { tag = configuration.rustTarget; binary = binary; };
      };
    };
  };

  environment = {
    variables = { targetPkgs, hostPkgs, }: {
        PKG_CONFIG_ALLOW_CROSS=true;
        # We must set some protoc related env vars for the prost crate.
        PROTOC = "${hostPkgs.protobuf}/bin/protoc"; # NOTE: `targetPkgs.pkgs` points to the 'host' packages.
        PROTOC_INCLUDE = "${hostPkgs.protobuf}/include"; # NOTE: `targetPkgs.pkgs` points to the 'host' packages.
        # On Linux builds, we need some level of localization.
        LOCALE_ARCHIVE = if targetPkgs.stdenv.isLinux && targetPkgs.glibcLocales != null then
          "${targetPkgs.glibcLocales}/lib/locale/locale-archive"
        else
          "";
        # LC_ALL = "en_US.UTF-8";
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
        # CPATH = if targetPkgs.stdenv.isLinux then
        #   "${targetPkgs.linuxHeaders}/include"
        # else
        #   "";
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
      depsBuildHost = passedPkgs:
        with passedPkgs.buildPackages;
          [
            pkg-config
            leveldb
            snappy
          ]
          ++ (
            if stdenv.isDarwin then
              [
                # TODO: These are probably in the wrong place.
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
          ) ++ (
            if passedPkgs.targetPlatform.libc == "glibc" then
              [
                glibc.static
              ]
            else if passedPkgs.targetPlatform.libc == "musl" then
              [
                musl
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
      depsBuildBuild = passedPkgs:
        with passedPkgs.buildPackages;
        [
            # This is required for rdkafka
            rdkafka
            openssl.dev
            jemalloc
            perl
            autoconf
            gcc
            gnumake
            zlib
            zstd
            libgssglue
            cmake
        ] ++ (
          if stdenv.isDarwin then
            []
          else if stdenv.isLinux then
            []
          else
            []
        );

      depsHostTarget = passedPkgs:
        with passedPkgs.buildPackages;
          [
          ];
      depsHostBuild = passedPkgs:
        with passedPkgs.buildPackages;
          [
          ];
      nativeBuildInputs = passedPkgs:
        with passedPkgs.buildPackages;
          [
          ];
    };
  };

  ### Development code.

  cargoToml = (builtins.fromTOML (builtins.readFile ./Cargo.toml));

  overlays = rec {
    mozilla = import (builtins.fetchGit {
        url = "https://github.com/mozilla/nixpkgs-mozilla/";
        rev = "e912ed483e980dfb4666ae0ed17845c4220e5e7c";
      });
    vector = import ./nix/overlay/default.nix;
  };

  pkgs = import <nixpkgs> {
    overlays = [
      overlays.mozilla
      overlays.vector
    ];
  };

  # Handy feature aliases for use in `configurations`
  features = {
    components = rec {
      sources = cargoToml.features.sources;
      sinks = cargoToml.features.sinks;
      transforms = cargoToml.features.transforms;
      all = sources ++ sinks ++ transforms;
      portable = sources ++ sinks ++
        # rlua fails on 
        (builtins.filter (val: val != "transforms-lua") transforms);
    };
    byLinking = {
      static = [ "rdkafka" "rdkafka-cmake" "vendored" ];
      dynamic = [ "rdkafka" "rdkafka/dynamic_linking" ];
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
      setInterpreterPath,
      features,
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
        packageDefinition = rec {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;

          depsBuildHost = (environment.dependencies.depsBuildHost targetPkgs);
          depsBuildBuild = (environment.dependencies.depsBuildBuild hostPkgs);
          depsHostTarget = (environment.dependencies.depsHostTarget targetPkgs);
          depsHostBuild = (environment.dependencies.depsHostBuild targetPkgs);
          nativeBuildInputs =  (environment.dependencies.nativeBuildInputs hostPkgs);

          passthru = (environment.variables { inherit (args) targetPkgs hostPkgs; });
          # Configurables
          buildType = args.buildType;
          logLevel = args.logLevel;
          cargoSha256 = "0xg43s4vdhzqz6gqbakr7c7jbr1jlmwr15ykrsl1clgywpg3rm8r";
          # TODO: There seems to be a cargoVendorDir option: https://github.com/NixOS/nixpkgs/blob/a7fa6f60c4df3fde0ab46cfe79294c1d65042fa4/pkgs/build-support/rust/default.nix#L30
          
          # We do this to make our builds portable to non-NixOS machines.
          # If `setInterpreterPath = false` the output binaries will refer to NixOS's `ld-linux` interpreter, this is a problem if you want to send this to others!
          # Instead, set it to `setInterpreterPath = "/lib64/ld-linux-x86-64.so.2"` and then Ubuntu/Centos can launch it.
          # 
          # Aren't computers fun?
          postFixup = (if (builtins.isString setInterpreterPath) then
              ''
                ${hostPkgs.patchelf}/bin/patchelf --set-interpreter ${setInterpreterPath} $out/bin/vector
              ''
            else
              "");

          target = args.rustTarget;
          # Rest
          src = hostPkgs.lib.cleanSource (tools.gitignore.gitignoreSource ./.);

          cargoBuildFlags = [ "--no-default-features" "--features" "${hostPkgs.lib.concatStringsSep "," features}" ];
          checkPhase = if runCheckPhase then
              ''
              # Configurables
              export TZDIR=${targetPkgs.tzdata}/share/zoneinfo
              cargo test --no-default-features --features ${hostPkgs.lib.concatStringsSep "," features} -- --test-threads 1
              ''
            else
              "";
          stdenv = pkgs.stdenvAdapters.makeStaticBinaries;
          
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
        } // (environment.variables { inherit (args) targetPkgs hostPkgs; });
      in
        (targetPkgs.makeRustPlatform {
          cargo = rustChannel.rust.override {
            targets = [ args.rustTarget ];
          };
          rustc = rustChannel.rust.override {
            targets = [ args.rustTarget ];
          };
          stdenv = pkgs.stdenvAdapters.makeStaticBinaries;
          # stdenv = overrideCC stdenv (stdenv.cc.override { bintools = stdenv.cc.bintools.override { libc = stdenv.libc; }; };
        }).buildRustPackage packageDefinition;
  };

  tools = {
      # naersk = import (builtins.fetchGit {
      #   url = "https://github.com/nmattia/naersk/";
      #   rev = "a82fd7dc31a58c462b6dfa9d9d886fa2cc75dfd4";
      # });
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