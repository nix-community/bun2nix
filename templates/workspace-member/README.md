# Bun2Nix Workspace Member Template

Build a single workspace member without pulling the entire root into the derivation.
This keeps each package's closure minimal — changes to one member don't invalidate
another's build, and each derivation only depends on the siblings it actually uses.

Compare with the `workspace` template, which builds from the workspace root and includes
everything.

## Structure

- `packages/app`: The application, with its own `default.nix`.
- `packages/lib`: A sibling library that `app` depends on.

## Building

```bash
nix build
```

## Development

```bash
nix develop
```
