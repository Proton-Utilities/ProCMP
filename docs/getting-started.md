# Getting Started

## Prerequisites

ProCMP depends on one tool:

- [**darklua**](https://github.com/seaofvoices/darklua) — bundler and minifier

Install via [Aftman](https://github.com/LPGhatguy/aftman) (recommended):

```toml title="aftman.toml"
[tools]
darklua = "Stefanuk12/darklua@0.17.4"
ProCMP  = "Proton-Utilities/ProCMP@3.0.0"
```

Then run:

```sh
aftman install
```

---

## Setting up a new project

The fastest way to get started is `pcmp init`:

```sh
pcmp init
```

This launches an interactive wizard that generates:

- `pipeline/.pcmp.json` — shared pipeline config (commit this)
- `pipeline/.pcmp.local.json` — your personal editor preferences (gitignored)
- `pipeline/frames/` — frame templates
- `pipeline/darklua/` — darklua configs (stable + unminified)
- `.env.example` — token template
- Updated `.gitignore`

See the full walkthrough in [pcmp init](init.md).

---

## Running the pipeline

```sh
pcmp pipeline/.pcmp.json
```

You'll be prompted to:

1. **Select a build configuration** (e.g. Debug, Beta, Release)
2. **Enter a version** (if `promptVersion: true`) — must match `versionFormat`
3. **Confirm public distribution** (if `promptDeploy: true`)

### CLI flags

| Flag | Description |
|------|-------------|
| `pcmp <config>` | Run pipeline with the given config file |
| `pcmp init` | Interactive project scaffolder |
| `pcmp --help` / `-h` | Show help |
| `pcmp --version` / `-V` | Print version string |

---

## Your first build

After running `pcmp init`, try a Debug build:

```sh
pcmp pipeline/.pcmp.json
# → Select: Debug
# → Build complete: generated/debug/myScript.luau
```

The output file contains your composed source, inserted into the frame with all composer markers replaced.

!!! note "Editor preview"
    If you set an editor in `pcmp init`, the composed output will open automatically after each build. This is stored in your gitignored `.pcmp.local.json`.
