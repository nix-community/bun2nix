{ config, ... }:
{
  perSystem =
    { pkgs, ... }:
    {
      packages.workspacePromoter = pkgs.rustPlatform.buildRustPackage {
        pname = "bun2nix-workspace-promoter";
        inherit (config.cargoTOML.package) version;

        src = ../../programs;

        cargoLock = {
          lockFile = ../../programs/Cargo.lock;
        };

        cargoBuildFlags = [
          "-p"
          "workspace-promoter"
        ];
        cargoTestFlags = [
          "-p"
          "workspace-promoter"
        ];

        doCheck = true;

        meta = {
          description = "Promote a bun workspace member to lockfile root";
          mainProgram = "workspace-promoter";
        };
      };
    };
}
