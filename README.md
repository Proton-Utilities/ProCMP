# ProCMP

Build composition and release pipeline for Luau projects.

**[→ Documentation](https://proton-utilities.github.io/ProCMP)**

---

ProCMP takes your source files, runs them through [darklua](https://github.com/seaofvoices/darklua) for bundling and minification, inserts them into a **frame** using composer markers, and optionally publishes the result directly to GitHub Releases — all from a single interactive CLI.

## Installation

```toml title="aftman.toml"
[tools]
darklua = "Stefanuk12/darklua@0.17.4"
lune    = "lune-org/lune@0.10.4"
ProCMP  = "Proton-Utilities/ProCMP@3.0.0"
```

```sh
aftman install
```

## Quick start

```sh
# Scaffold a new project
pcmp init

# Run the pipeline
pcmp pipeline/.pcmp.json
```

## CLI

```
pcmp <config>    Run pipeline with the given config file
pcmp init        Interactive project scaffolder
pcmp --help      Show help
pcmp --version   Print version
```

## Team usage

ProCMP separates **shared config** (committed) from **personal preferences** (gitignored):

- `pipeline/.pcmp.json` — build configs, deployment settings → commit this
- `pipeline/.pcmp.local.json` — your editor, output opener → each developer sets their own

Each team member runs `pcmp init` once to configure their personal local config.

## Documentation

Full documentation at **[proton-utilities.github.io/ProCMP](https://proton-utilities.github.io/ProCMP)**.

Topics covered:

- [Getting started](https://proton-utilities.github.io/ProCMP/getting-started)
- [pcmp init](https://proton-utilities.github.io/ProCMP/init)
- [Local config](https://proton-utilities.github.io/ProCMP/local-config)
- [Composer markers](https://proton-utilities.github.io/ProCMP/composer-markers)
- [Deployment](https://proton-utilities.github.io/ProCMP/deployment)
- [Config reference](https://proton-utilities.github.io/ProCMP/config-reference)
