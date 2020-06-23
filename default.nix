rec {
  target = {
    # Output Artifacts
    releases = rec {
      x86_64-unknown-linux-gnu = rec {
        binary = tasks.binary configurations.x86_64-unknown-linux-gnu;
        docker = tasks.docker { tag = "gnu"; binary = binary; };
        all = [ binary docker ];
      };
      x86_64-unknown-linux-musl = rec {
        binary = tasks.binary configurations.x86_64-unknown-linux-musl;
        docker = tasks.docker { tag = "musl"; binary = binary; };
        all = [ binary docker ];
      };
      x86_64-pc-windows-gnu = rec {
        binary = tasks.binary configurations.x86_64-pc-windows-gnu;
        all = [ binary ];
      };
      all = [ x86_64-unknown-linux-gnu x86_64-unknown-linux-musl ];
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

  # Available compile configurations for Vector, leading to their config input for `binary`
  configurations = rec {
    # See `rustup target list`
    x86_64-unknown-linux-gnu = {
      buildType = "debug";
      rustTarget = "x86_64-unknown-linux-gnu";
      pkgs = pkgs;
      cross = if pkgs.targetPlatform.config == pkgs.pkgsCross.gnu64.targetPlatform.config then
          null
        else
          pkgs.pkgsCross.gnu64;
      logLevel = "debug";
      runCheckPhase = false;
      features = features.components.all ++
        features.byOs.linux.gnu ++
        features.byLinking.dynamic;
    };
    x86_64-unknown-linux-musl = {
      buildType = "debug";
      rustTarget = "x86_64-unknown-linux-musl";
      pkgs = pkgs;
      cross = if pkgs.targetPlatform.config == pkgs.pkgsCross.musl64.targetPlatform.config then
          null
        else
          pkgs.pkgsCross.musl64;
      logLevel = "debug";
      runCheckPhase = false;
      features = features.components.all ++
        features.byOs.linux.musl;
    };
    x86_64-pc-windows-gnu = {
      buildType = "debug";
      rustTarget = "x86_64-pc-windows-gnu";
      pkgs = pkgs;
      cross = if pkgs.targetPlatform.config == pkgs.pkgsCross.darwin.targetPlatform.config then
          null
        else
          pkgs.pkgsCross.darwin;
      logLevel = "debug";
      runCheckPhase = false;
      features = features.components.all ++
        features.byOs.linux.gnu ++
        features.byLinking.dynamic;
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
      pkgs ? pkgs,
      cross ? null,
      buildType ? "debug",
      logLevel ? "debug",
      runCheckPhase ? true,
    }:
      let
        rustChannel = (pkgs.rustChannelOf {
          rustToolchain = ./rust-toolchain;
          targets = [ args.rustTarget ];
          extensions = [
            "rust-std"
          ];
          targetExtensions = [
            "rust-std"
          ];
        });
        definition = import ./scripts/environment/definition.nix { inherit tools pkgs rustTarget cross; };
        features = (builtins.getAttr args.rustTarget configurations).features;
        
        packageDefinition = rec {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;
          # See `definition.nix` for details on these.
          nativeBuildInputs = definition.nativeBuildInputs;
          buildInputs = definition.buildInputs;
          passthru = definition.environmentVariables;
          # Configurables
          buildType = args.buildType;
          logLevel = args.logLevel;
          cargoSha256 = "0yld7dczz27i5bzi39pr1fxxxfmdaqxl80hymxbqxdnwhhq0hjch";
          
          target = args.rustTarget;
          # Rest
          src = pkgs.lib.cleanSource (tools.gitignore.gitignoreSource ./.);

          cargoBuildFlags = [ "--no-default-features" "--features" "${pkgs.lib.concatStringsSep "," features}" ];
          checkPhase = if runCheckPhase then
              ''
              # Configurables
              export TZDIR=${pkgs.tzdata}/share/zoneinfo
              cargo test --no-default-features --features ${pkgs.lib.concatStringsSep "," features} -- --test-threads 1
              ''
            else
              "";
          
          # cargoBuildOptions = currentOptions: currentOptions ++ [ "--no-default-features" "--features" "${pkgs.lib.concatStringsSep "," features}" ];
          # cargoTestOptions = currentOptions: currentOptions ++ [ "--no-default-features" "--features" "${pkgs.lib.concatStringsSep "," features}" ];
          # cargoTestCommands = currentOptions: if runCheckPhase then
          #   currentOptions
          # else
          #   [];
          meta = with pkgs.stdenv.lib; {
            description = "A high-performance logs, metrics, and events router";
            homepage    = "https://github.com/timberio/vector";
            license = pkgs.lib.licenses.asl20;
            maintainers = [];
            platforms = platforms.all;
          };
        } // definition.environmentVariables;
      in
        # (tools.naersk.buildPackage packageDefinition);
        ((if args ? cross then cross else pkgs).makeRustPlatform {
          cargo = rustChannel.cargo;
          rustc = rustChannel.rustc;
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