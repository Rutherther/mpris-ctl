{
  description = "Mpris control utility that keeps track of the last playing player";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    naersk = {
      url = "github:nmattia/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, naersk, nixpkgs }: let
    cargoToml = (builtins.fromTOML (builtins.readFile ./Cargo.toml));
    supportedSystems = ["x86_64-linux"];
    forAllSystems = f: nixpkgs.lib.genAttrs supportedSystems (system: f system);
  in {
    overlay = final: prev: {
      "${cargoToml.package.name}" = final.callPackage ./. { inherit naersk; };
    };

    packages = forAllSystems (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            self.overlay
          ];
        };
      in {
        "${cargoToml.package.name}" = pkgs."${cargoToml.package.name}";
      });

    defaultPackage = forAllSystems (system:
      self.packages."${system}"."${cargoToml.package.name}"
    );

    devShell = forAllSystems (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [
          self.overlay
        ];
      };
    in pkgs.mkShell {
      inputsFrom = [
        pkgs."${cargoToml.package.name}"
      ];

      buildInputs = [
        pkgs.rustfmt
        pkgs.nixpkgs-fmt
      ];
    });
  };
}
