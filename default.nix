rec {
  binaries = rec {
    native = binary {
      linking = "dynamic";
      buildType = "debug";
      rustTarget = "x86_64-unknown-linux-gnu";
      logLevel = "debug";
      runCheckPhase = false;
    };
    x86_64-unknown-linux-gnu = binary {
      linking = "dynamic";
      buildType = "debug";
      rustTarget = "x86_64-unknown-linux-gnu";
      crossSystem = (import <nixpkgs>).lib.systems.examples.gnu64;
      logLevel = "debug";
      runCheckPhase = false;
    };
    x86_64-unknown-linux-musl = binary {
      linking = "static";
      buildType = "debug";
      rustTarget = "x86_64-unknown-linux-musl";
      crossSystem = (import <nixpkgs>).lib.systems.examples.musl64;
      logLevel = "debug";
      runCheckPhase = false;
    };
  };

  binary = args@{ 
    features ? null,
    linking ? "dynamic",
    rustChannel ? null, # Defaulted below.
    rustTarget ? "x86_64-unknown-linux-gnu",
    crossSystem ? builtins.currentSystem,
    buildType ? "debug",
    logLevel ? "debug",
    runCheckPhase ? true,
  }:
    let
      pkgs = import <nixpkgs> { 
          overlays = [
            (import (builtins.fetchTarball https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz))
          ];
        } //
          (if args ? crossSystem then {
            crossSystem = {
              config = crossSystem;
            };
          } else {});

      definition = import ./scripts/environment/definition.nix (args // pkgs);
      features = builtins.getAttr args.rustTarget definition.features.presets;
      
      packageDefinition = rec {
        # See `definition.nix` for details on these.
        pname = definition.cargoToml.package.name;
        version = definition.cargoToml.package.version;
        nativeBuildInputs = definition.nativeBuildInputs;
        buildInputs = definition.buildInputs;
        passthru = definition.environmentVariables;
        # Configurables
        buildType = args.buildType;
        logLevel = args.logLevel;
        
        target = args.rustTarget;
        # Rest
        src = ./.;
        cargoSha256 = "062bq8jzgxp822870zgaiqg3i7i2vi0nfggl8nrhpbphfbqn21a5";
        verifyCargoDeps = true;
        cargoBuildFlags = [ "--no-default-features" "--features" "${pkgs.lib.concatStringsSep "," features}" ];
        checkPhase = if runCheckPhase then
          ''
            export TZDIR=${pkgs.tzdata}/share/zoneinfo
            cargo test --no-default-features --features ${pkgs.lib.concatStringsSep "," features} -- --test-threads 1
          ''
        else
          "";
        meta = with pkgs.stdenv.lib; {
          description = "A high-performance logs, metrics, and events router";
          homepage    = "https://github.com/timberio/vector";
          license = pkgs.lib.licenses.asl20;
          maintainers = [];
          platforms = platforms.all;
        };
      } // definition.environmentVariables;
    in
      pkgs.rustPlatform.buildRustPackage packageDefinition;
}