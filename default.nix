# This part defines the **function inputs. Anything without a `? ...` must be passed in.
scope@{
  pkgs ? import <nixpkgs> {},
  features ? ["default-nix"],
}:

# Here we set some aliases.
let
  # This is our shared definitions for environments (shells, builds, etc).
  definition = (import ./scripts/environment/definition.nix scope);

  # Fill in some data about this package in the format expected.
  crateBaseDefinition = rec {
    pname = definition.cargoToml.package.name;
    version = definition.cargoToml.package.version;

    buildType = "debug";
    logLevel = "debug";

    src = ./.;

    # See `definition.nix` for details on these.
    nativeBuildInputs = definition.nativeBuildInputs;
    buildInputs = definition.buildInputs;

    cargoSha256 = "1nk15xv33f1qilq2187k5gj68bhkp67jvs8nawmawb0yrww5dcnv";
    verifyCargoDeps = true;

    passthru = definition.environmentVariables;

    cargoBuildFlags = [ "--no-default-features" "--features" "${pkgs.lib.concatStringsSep "," features}" ];
    checkPhase = "cargo test --no-default-features --features ${pkgs.lib.concatStringsSep "," features},disable-resolv-conf -- --test-threads 1";

    meta = with pkgs.stdenv.lib; {
      description = "A high-performance logs, metrics, and events router";
      homepage    = "https://github.com/timberio/vector";
      license = pkgs.lib.licenses.asl20;
      maintainers = [];
      platforms = platforms.all;
    };
  } // definition.environmentVariables;

  deriveDefinition = extension: crateBaseDefinition // extension;

  build = crateDefinition:
    builtins.trace crateDefinition (pkgs.rustPlatform.buildRustPackage crateDefinition);
in

# Outputs
{
  packet = build (deriveDefinition {});
}