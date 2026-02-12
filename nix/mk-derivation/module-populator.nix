{ config, ... }:
{
  perSystem =
    { pkgs, ... }:
    {
      packages.modulePopulator = pkgs.rustPlatform.buildRustPackage {
        pname = "bun2nix-module-populator";
        inherit (config.cargoTOML.package) version;

        src = ../../programs;

        cargoLock = {
          lockFile = ../../programs/Cargo.lock;
        };

        cargoBuildFlags = [
          "-p"
          "module-populator"
        ];
        cargoTestFlags = [
          "-p"
          "module-populator"
        ];

        doCheck = true;

        meta = {
          description = "Construct node_modules from bun cache without network access";
          mainProgram = "module-populator";
        };
      };
    };
}
