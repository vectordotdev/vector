rec {
  # Output Artifacts
  releases = rec {
    binaries = rec {
      x86_64-unknown-linux-gnu = tasks.binary targets.x86_64-unknown-linux-gnu;
      x86_64-unknown-linux-musl = tasks.binary targets.x86_64-unknown-linux-musl;
    };
  };

  ### Development code.

  cargoToml = (builtins.fromTOML (builtins.readFile ./Cargo.toml));

  overlays = rec {
    mozilla = (import (builtins.fetchTarball https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz));
  };

  pkgs = import <nixpkgs> {
    overlays = [
      overlays.mozilla
    ];
  };

  # Handy feature aliases for use in `targets`
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
        gnu = [ "unix" ];
        musl = [ ];
      };
      mac = ["unix"];
      windows = [];
      freebsd = [];
    };
  };

  # Available compile targets for Vector, leading to their config input for `binary`
  targets = rec {
    # See `rustup target list`
    x86_64-unknown-linux-gnu = {
      linking = "dynamic";
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
      linking = "dynamic";
      buildType = "debug";
      rustTarget = "x86_64-unknown-linux-musl";
      pkgs = pkgs;
      cross = if pkgs.targetPlatform.config == pkgs.pkgsCross.musl64.targetPlatform.config then
          pkgs
        else if pkgs.targetPlatform.config == pkgs.pkgsCross.gnu64.targetPlatform.config then
          pkgs
        else 
          pkgs.pkgsCross.musl64;
      logLevel = "debug";
      runCheckPhase = false;
      features = features.components.all ++
        features.byOs.linux.musl;
    };
  };
  
  # Jobs used to build artifacts
  tasks = rec {
    # Build a binary Vector artifact
    binary = args@{
      # This will be set dynamically!
      features ? null,
      linking ? "dynamic",
      rustChannel ? null, # Defaulted below
      rustTarget ? null,
      pkgs ? pkgs,
      cross ? null,
      buildType ? "debug",
      logLevel ? "debug",
      runCheckPhase ? true,
    }:
      let

        definition = import ./scripts/environment/definition.nix args;
        features = (builtins.getAttr args.rustTarget targets).features;
        
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
          
          target = args.rustTarget;
          # Rest
          root = ./.;
          cargoBuildOptions = currentOptions: currentOptions ++ [ "--no-default-features" "--features" "${pkgs.lib.concatStringsSep "," features}" ];
          cargoTestOptions = currentOptions: currentOptions ++ [ "--no-default-features" "--features" "${pkgs.lib.concatStringsSep "," features}" ];
          cargoTestCommands = currentOptions: if runCheckPhase then
            currentOptions
          else
            [];
          meta = with pkgs.stdenv.lib; {
            description = "A high-performance logs, metrics, and events router";
            homepage    = "https://github.com/timberio/vector";
            license = pkgs.lib.licenses.asl20;
            maintainers = [];
            platforms = platforms.all;
          };
        } // definition.environmentVariables;
      in
        (tools.naersk.buildPackage packageDefinition);
        #pkgs.rustPlatform.buildRustPackage packageDefinition;
  };

  tools = {
    naersk = pkgs.callPackage (builtins.fetchTarball https://github.com/nmattia/naersk/archive/master.tar.gz) {};
  };
}