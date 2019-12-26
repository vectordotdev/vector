# Taken from https://github.com/nixos/nixpkgs

{ stdenv, lib, fetchFromGitHub, rustPlatform
, openssl, pkgconfig, protobuf
, Security, libiconv, rdkafka, cmake

, features ?
    (if stdenv.isAarch64
     then [ "shiplift/unix-socket" "jemallocator" "rdkafka" "rdkafka/dynamic_linking" ]
     else [ "leveldb" "leveldb/leveldb-sys-2" "shiplift/unix-socket" "jemallocator" "rdkafka" "rdkafka/dynamic_linking" ])
}:

rustPlatform.buildRustPackage rec {
  pname = "vector";
  version = "<%=
    require 'toml-rb'
    TomlRB.load_file('Cargo.toml')['package']['version']
  %>";

  src = <%= Dir.getwd %>; # TODO: allow using GitHub as well
  # src = fetchFromGitHub {
  #   owner  = "timberio";
  #   repo   = pname;
  #   rev    = "refs/tags/v${version}";
  #   sha256 = "0bb4552nwkdpnxhaq2mn4iz5w92ggqxc1b78jq2vjbh1317sj9hw";
  # };

  cargoSha256 = "0igag0v5m58bx1p5zdy6pzv8k7lyq12l9ix86x5m6d08fvcfalyf"; # TODO: use a template
  buildInputs = [ openssl pkgconfig protobuf rdkafka cmake ]
                ++ stdenv.lib.optional stdenv.isDarwin [ Security libiconv ];

  # needed for internal protobuf c wrapper library
  PROTOC="${protobuf}/bin/protoc";
  PROTOC_INCLUDE="${protobuf}/include";

  cargoBuildFlags = [ "--no-default-features" "--features" "${lib.concatStringsSep "," features}" ];
  checkPhase = ":"; # skip tests, too -- they don't respect the rdkafka flag...

  meta = with stdenv.lib; {
    description = "A high-performance logs, metrics, and events router";
    homepage    = "https://github.com/timberio/vector";
    license     = with licenses; [ asl20 ];
    maintainers = with maintainers; [ thoughtpolice ];
    platforms   = platforms.all;
  };
}
