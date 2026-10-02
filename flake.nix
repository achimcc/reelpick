{
  description = "reelpick — one film a day from your Jellyfin library";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  # The RustSec advisory database, pinned like any other input. The `audit`
  # check reads it offline; `nix flake update advisory-db` brings news in.
  inputs.advisory-db = {
    url = "github:rustsec/advisory-db";
    flake = false;
  };

  outputs =
    { self, nixpkgs, advisory-db }:
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
        let
          package = self.packages.${pkgs.system}.default;
        in
        {
          inherit package;
          # Known advisories against Cargo.lock, read offline from the pinned
          # database.
          audit = pkgs.runCommand "reelpick-audit" { nativeBuildInputs = [ pkgs.cargo-audit ]; } ''
            HOME=$TMPDIR cargo-audit audit --no-fetch --db ${advisory-db} --file ${./Cargo.lock}
            touch $out
          '';
          # Bans, sources and licenses of the dependency tree (deny.toml).
          # Inside the package's build environment: the vendored crates are
          # what `cargo metadata` reads there, so nothing is fetched.
          deny = package.overrideAttrs (old: {
            pname = "reelpick-deny";
            nativeBuildInputs = old.nativeBuildInputs ++ [ pkgs.cargo-deny ];
            buildPhase = "cargo deny --offline check bans sources licenses";
            doCheck = false;
            installPhase = "touch $out";
          });
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
