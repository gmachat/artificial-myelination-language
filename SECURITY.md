# Security policy

## Supported version

Security fixes currently target the latest AML development release.

## Reporting

Do not open a public issue for a suspected vulnerability. Send a private
GitHub security advisory to the repository maintainers with a reproduction,
affected version, and impact. Maintainers should acknowledge a report within
seven days and coordinate disclosure after a fix is available.

## Trust boundaries

- AML source, external backends, generated projects, and model-produced code
  are untrusted input.
- `aml check` and `aml build` do not execute AML source.
- `aml run` deliberately executes generated code and must not be used on
  untrusted source outside an appropriate sandbox.
- External backend executables are native programs with the invoking user's
  permissions. Only configure backends you trust.
- The MCP server confines declared source and output paths to its canonical
  `--project` root and rejects symbolic-link traversal. Its `format` and `build`
  tools are intentional write operations and should remain subject to host
  approval controls.
- The AI workflow benchmark launches the configured adapter as a native process.
  Use only adapter executables you trust. Structural evaluation never executes
  model-generated TypeScript. Runtime evaluation is an explicit `--execute`
  opt-in and runs generated applications in a restricted Docker or Podman
  container. The dependency-install phase has network access, so use a
  disposable host for deliberately adversarial submissions.
- Generated web dependencies and runtime toolchains have their own security
  update lifecycle and are not embedded in the AML compiler.

