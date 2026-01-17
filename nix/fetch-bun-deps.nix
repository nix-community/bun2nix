{ lib, flake-parts-lib, ... }:
let
  inherit (flake-parts-lib) mkPerSystemOption;
  inherit (lib) mkOption types;

  invalidBunNixErr = ''
    Your supplied `bun.nix` dependencies file failed to evaluate.

    This is likely because the version of `bun2nix` you are using has changed and
    the `bun.nix` file has no schema stability guarantees between versions, and
    will simply change as needed since updating it is trivial.

    As a result, you should try regenerating your `bun.nix` file:

    ```sh
    bun2nix -o bun.nix
    ```
  '';
in
{
  options.perSystem = mkPerSystemOption {
    options.fetchBunDeps.function = mkOption {
      description = ''
        Bun cache creator function.

        Produces a file accurate, symlink farm recreation of bun's global install cache.

        See [bun's cache docs](https://github.com/oven-sh/bun/blob/642d04b9f2296ae41d842acdf120382c765e632e/docs/install/cache.md#L24)
        for more information.
      '';
      type = types.functionTo types.package;
    };
  };

  config.perSystem =
    { pkgs, config, ... }:
    {
      fetchBunDeps.function =
        {
          bunNix,
          overrides ? { },
          # Map of package names to patch file paths, e.g.:
          # { "lodash@4.17.21" = ./patches/lodash@4.17.21.patch; }
          patchedDependencies ? { },
          ...
        }@args:
        let
          attrIsBunPkg = _: value: lib.isStorePath value;

          withErrCtx = builtins.addErrorContext invalidBunNixErr (pkgs.callPackage bunNix { });

          packages = lib.filterAttrs attrIsBunPkg withErrCtx;

          buildPackage = config.fetchBunDeps.buildPackage args;
          overridePackage = config.fetchBunDeps.overridePackage args;
        in

        assert lib.asserts.assertEachOneOf "overrides" (builtins.attrNames overrides) (
          builtins.attrNames packages
        );

        assert lib.assertMsg (builtins.all builtins.isFunction (builtins.attrValues overrides))
          "All attr values of `overrides` must be functions taking the old, unoverrided package and returning the new source.";

        pkgs.symlinkJoin {
          name = "bun-cache";
          paths = lib.pipe packages [
            (builtins.mapAttrs overridePackage)
            (builtins.mapAttrs buildPackage)
            builtins.attrValues
          ];
        };
    };
}
