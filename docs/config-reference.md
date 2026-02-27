# Config Reference

Full reference for `.pcmp.json` and `.pcmp.local.json`.

---

## `.pcmp.json` — Shared cfg

This file is committed to source control and shared across all team members.

### Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `envFile` | `string` | `".env"` | Path to the env file for reading tokens |
| `openComposedOutput` | [`OpenFileConfig`](#openfileconfig) | — | Open composed output after each build (prefer local config) |
| `releaseNotesEditor` | [`OpenFileConfig`](#openfileconfig) | — | Editor for release notes (prefer local config) |
| `deployment` | [`DeploymentConfig`](#deploymentconfig) | — | GitHub deployment settings |
| `buildConfigs` | [`BuildConfig[]`](#buildconfig) | **required** | Array of named build configurations |

---

### `BuildConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | `string` | **required** | Display name shown in the selection prompt |
| `input` | `string` | — | Source entry point (mutually exclusive with `inputDir`) |
| `inputDir` | `string` | — | Scan a directory for `.luau`/`.lua` files |
| `output` | `string` | — | Output file path (single-file mode) |
| `outputDir` | `string` | — | Output directory (directory mode) |
| `frame` | `string` | **required** | Path to the frame file |
| `darkluaConfig` | `string` | **required** | Path to the darklua config |
| `promptVersion` | `boolean` | `false` | Prompt for a version string before building |
| `versionFormat` | `string` | `"^v%d+%.%d+%.%d+"` | Lua pattern the version string must match |
| `promptDeploy` | `boolean` | `false` | Prompt to deploy after building |
| `prerelease` | `boolean` | `false` | Mark the GitHub release as a pre-release |
| `openComposedOutput` | [`OpenFileConfig`](#openfileconfig) | — | Per-config output opener (overrides global) |
| `variants` | [`BuildVariant[]`](#buildvariant) | — | Additional builds that inherit this config |

#### Directory mode (`inputDir`)

When `inputDir` is set, ProCMP scans the directory for `.luau` and `.lua` files and builds each one independently. Useful for test suites.

```json
{
    "name": "Tests",
    "inputDir": "tests",
    "outputDir": "generated/tests",
    "frame": "pipeline/frames/release.luau",
    "darkluaConfig": "pipeline/darklua/unminified.json"
}
```

#### Single-file mode (`input`)

Standard mode: one source file → one output file.

```json
{
    "name": "Release",
    "input": "src/init.luau",
    "output": "generated/release/myScript.luau",
    "frame": "pipeline/frames/release.luau",
    "darkluaConfig": "pipeline/darklua/stable.json"
}
```

---

### `BuildVariant`

Variants inherit all fields from their parent `BuildConfig` and override any fields you specify. They are built in sequence after the parent.

| Field | Type | Description |
|-------|------|-------------|
| `name` | `string` | Display name for the variant |
| `input` | `string` | Override source entry point |
| `output` | `string` | Override output path |
| `frame` | `string` | Override frame |
| `darkluaConfig` | `string` | Override darklua config |

```json
{
    "name": "Release",
    "input": "src/init.luau",
    "output": "generated/release/myScript.luau",
    "frame": "pipeline/frames/release.luau",
    "darkluaConfig": "pipeline/darklua/stable.json",
    "variants": [
        {
            "name": "Release-Unminified",
            "darkluaConfig": "pipeline/darklua/unminified.json",
            "output": "generated/release/myScript-unminified.luau"
        }
    ]
}
```

---

### `DeploymentConfig`

```json
{
    "deployment": {
        "github": {
            "owner": "your-username",
            "repo": "your-repo",
            "tokenEnvVar": "GITHUB_TOKEN"
        }
    }
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `github.owner` | `string` | **required** | GitHub username or organisation |
| `github.repo` | `string` | **required** | Repository name |
| `github.tokenEnvVar` | `string` | `"GITHUB_TOKEN"` | Env var name to read the token from |

---

### `OpenFileConfig`

Used for `openComposedOutput` and `releaseNotesEditor`.

| Field | Type | Description |
|-------|------|-------------|
| `command` | `string` | Executable to run |
| `args` | `string[]` | Arguments — `{file}` is replaced with the file path |

```json
{
    "command": "code",
    "args": ["--wait", "{file}"]
}
```

---

## `.pcmp.local.json` — Local config

Personal preferences

| Field | Type | Description |
|-------|------|-------------|
| `envFile` | `string` | Override env file path |
| `openComposedOutput` | [`OpenFileConfig`](#openfileconfig) | Your editor for output preview |
| `releaseNotesEditor` | [`OpenFileConfig`](#openfileconfig) | Your editor for release notes |

See [Local Config](local-config.md) for full documentation.

---

## Full example

```json title="pipeline/.pcmp.json"
{
    "deployment": {
        "github": {
            "owner": "your-name",
            "repo": "your-repo",
            "tokenEnvVar": "GITHUB_TOKEN"
        }
    },
    "buildConfigs": [
        {
            "name": "Tests",
            "inputDir": "tests",
            "outputDir": "generated/tests",
            "frame": "pipeline/frames/release.luau",
            "darkluaConfig": "pipeline/darklua/unminified.json"
        },
        {
            "name": "Debug",
            "input": "src/init.luau",
            "output": "generated/debug/myScript.luau",
            "frame": "pipeline/frames/debug.luau",
            "darkluaConfig": "pipeline/darklua/unminified.json"
        },
        {
            "name": "Release",
            "input": "src/init.luau",
            "output": "generated/release/myScript.luau",
            "frame": "pipeline/frames/release.luau",
            "darkluaConfig": "pipeline/darklua/stable.json",
            "promptVersion": true,
            "versionFormat": "^v%d+%.%d+%.%d+",
            "promptDeploy": true,
            "variants": [
                {
                    "name": "Release-Unminified",
                    "darkluaConfig": "pipeline/darklua/unminified.json",
                    "output": "generated/release/myScript-unminified.luau"
                }
            ]
        }
    ]
}
```
