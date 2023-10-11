{ lib
, stdenv
, clangStdenv
, rustPlatform
, hostPlatform
, targetPlatform
, pkg-config
, dbus
, rustfmt
, cargo
, rustc
}:

let
  cargoToml = (builtins.fromTOML (builtins.readFile ./Cargo.toml));
in
rustPlatform.buildRustPackage rec {
  name = "${cargoToml.package.name}";
  version = "${cargoToml.package.version}";

  src = ./.;
  cargoHash = "sha256-ZpQXclS9jota0IqQBmvTNp1JXZOq0xD7dAP1k9Cr9ok=";

  nativeBuildInputs = [
    rustfmt
    pkg-config
    cargo
    rustc
    dbus
  ];

  checkInputs = [ cargo rustc dbus ];
  doCheck = true;

  meta = {
    description = cargoToml.package.description;
    homepage = cargoToml.package.homepage;
    license = [ lib.licenses.mit ];
    maintainers = [];
  };
}
