{ lib, flake-parts-lib, ... }:
let
  inherit (flake-parts-lib) mkPerSystemOption;
  inherit (lib) mkOption types;
in
{
  options.perSystem = mkPerSystemOption {
    options.fetchBunDeps.buildPackage = mkOption {
      description = ''
        If the package is a tarball, extract it,
        otherwise make a copy of the directory in $out/share/bun-packages.

        If `patchShebangs` is enabled patch all
        scripts to use bun as their executor.

        Then, produce a bun cache compatible symlink in $out/share/bun-cache.
      '';
      type = types.functionTo (types.functionTo (types.functionTo types.package));
    };
  };

  config.perSystem =
    {
      pkgs,
      config,
      self',
      ...
    }:
    {
      fetchBunDeps.buildPackage =
        {
          patchShebangs ? true,
          autoPatchElf ? false,
          nativeBuildInputs ? [ ],
          ...
        }@args:
        let
          bunWithNode = config.fetchBunDeps.bunWithNode args;
        in
        name: pkg:
        pkgs.stdenv.mkDerivation {
          name = "bun-pkg-${name}";

          nativeBuildInputs = [
            bunWithNode
          ]
          ++ lib.optionals autoPatchElf (
            with pkgs;
            [
              autoPatchelfHook
              stdenv.cc.cc.lib
            ]
          )
          ++ nativeBuildInputs;

          phases = [
            "extractPhase"
            "patchPhase"
            "cacheEntryPhase"
          ];

          extractPhase = ''
            runHook preExtract

            "${lib.getExe config.fetchBunDeps.extractPackage}" \
              --package "${pkg}" \
              --out "$out/share/bun-packages/${name}"

            runHook postExtract
          '';

          patchPhase = ''
            runHook prePatch

            ${lib.optionalString patchShebangs ''patchShebangs "$out/share/bun-packages"''}
            ${lib.optionalString autoPatchElf ''runHook autoPatchelfPostFixup''}

            runHook postPatch
          '';

          cacheEntryPhase = ''
            runHook preCacheEntry

            "${lib.getExe self'.packages.cacheEntryCreator}" \
              --out "$out/share/bun-cache" \
              --name "${name}" \
              --package "$out/share/bun-packages/${name}"

            runHook postCacheEntry
          '';

          preferLocalBuild = true;
          allowSubstitutes = false;
        };
    };
}
