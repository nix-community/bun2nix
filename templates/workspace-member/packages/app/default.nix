{
  bun2nix,
  ...
}:
bun2nix.mkDerivation {
  pname = "workspace-test-app";
  version = "1.0.0";

  src = ./.;

  bunDeps = bun2nix.fetchBunDeps {
    bunNix = ../../bun.nix;
  };

  bunLockFile = ../../bun.lock;

  bunWorkspace = "packages/app";

  bunWorkspaceDeps = {
    "@workspace/lib" = ../lib;
  };

  module = "index.js";
}
