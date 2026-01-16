{
  description = "Bun2Nix private registry example";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    systems.url = "github:nix-systems/default";

    bun2nix.url = "github:nix-community/bun2nix?tag=2.0.6";
    bun2nix.inputs.nixpkgs.follows = "nixpkgs";
    bun2nix.inputs.systems.follows = "systems";
  };

  # Use the cached version of bun2nix from the nix-community cli
  nixConfig = {
    extra-substituters = [
      "https://cache.nixos.org"
      "https://nix-community.cachix.org"
    ];
    extra-trusted-public-keys = [
      "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
      "nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs="
    ];
  };

  outputs =
    inputs:
    let
      # Read each system from the nix-systems input
      eachSystem = inputs.nixpkgs.lib.genAttrs (import inputs.systems);

      # Access the package set for a given system
      pkgsFor = eachSystem (
        system:
        import inputs.nixpkgs {
          inherit system;
          # Use the bun2nix overlay, which puts `bun2nix` in pkgs
          overlays = [ inputs.bun2nix.overlays.default ];
        }
      );
    in
    {
      packages = eachSystem (system: {
        # Produce a package for this template with bun2nix in
        # the overlay
        default = pkgsFor.${system}.callPackage ./default.nix { };
      });

      devShells = eachSystem (system: {
        default = pkgsFor.${system}.mkShell {
          packages = with pkgsFor.${system}; [
            bun

            # Add the bun2nix binary to our devshell
            bun2nix
          ];

          shellHook = ''
            bun install --frozen-lockfile
          '';
        };
      });
    };
}
