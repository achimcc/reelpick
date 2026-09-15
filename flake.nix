{
  description = "reelpick — one film a day from your Jellyfin library";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs =
    { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (s: f nixpkgs.legacyPackages.${s});
    in
    {
      packages = forAll (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "reelpick";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
          # The tests resolve `Europe/Berlin`; the build sandbox has no zoneinfo.
          nativeCheckInputs = [ pkgs.tzdata ];
          preCheck = ''export TZDIR=${pkgs.tzdata}/share/zoneinfo'';
          meta = {
            description = "One film a day from your Jellyfin library, chosen and described by a local model";
            license = pkgs.lib.licenses.agpl3Only;
            mainProgram = "reelpick";
          };
        };
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [ cargo rustc rustfmt clippy rust-analyzer sqlite ];
        };
      });

      nixosModules.default = ./nix/module.nix;

      checks = forAll (
        pkgs:
        {
          package = self.packages.${pkgs.system}.default;
        }
        // nixpkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          vm = import ./nix/test.nix {
            inherit pkgs;
            module = self.nixosModules.default;
            package = self.packages.${pkgs.system}.default;
          };
        }
      );
    };
}
