---
description: Every field you can write, and what it does.
---

# Manifest

`pcmp` searches the working directory and then each directory above it. The extension picks the format, and all five produce the same build.

```
pcmp.json5   pcmp.json   pcmp.jsonc   pcmp.toml   pcmp.luau
```

Paths resolve against the manifest's directory, not the directory you ran from.

=== "pcmp.json5"

    ```json5
    {
      vars: { name: "app" },
      profiles: {
        release: {
          entry: "src/init.luau",
          output: "dist/{name}.luau",
          define: { DEBUG: false },
          darklua: { generator: "dense", rules: ["compute_expression"] },
        },
      },
    }
    ```

=== "pcmp.toml"

    ```toml
    [vars]
    name = "app"

    [profiles.release]
    entry  = "src/init.luau"
    output = "dist/{name}.luau"
    define = { DEBUG = false }

    [profiles.release.darklua]
    generator = "dense"
    rules     = ["compute_expression"]
    ```

=== "pcmp.luau"

    ```lua
    return {
    	vars = { name = "app" },
    	profiles = {
    		release = {
    			entry   = "src/init.luau",
    			output  = "dist/{name}.luau",
    			define  = { DEBUG = false },
    			darklua = { generator = "dense", rules = { "compute_expression" } },
    		},
    	},
    }
    ```

## Top level

| Key | What it holds |
| --- | --- |
| `vars` | named values every profile starts from |
| `templates` | never built, and exist to be extended |
| `profiles` | built, once each, or once per axis combination |

`extends` finds a name in either map, so a profile can extend a profile. A name may not appear in both.

## Profile fields

| Field | What it does |
| --- | --- |
| `extends` | a template or profile to inherit from |
| `entry` | a file to bundle, or a directory to process as a tree |
| `output` | where it goes |
| `sources` | extra files and directories that count as build inputs |
| `ignore` | globs excluded from that input set |
| `vars` | named values |
| `define` | constants substituted into your source |
| `header` | lines written above each artifact |
| `loaders` | an ordered list of `pattern` to `use` pairs |
| `darklua` | [darklua's own configuration](darklua.md), verbatim |
| `axes` | expands the profile into one task per combination |

Down an `extends` chain the nearest declaration wins. `vars` and `define` accumulate, `darklua` merges key by key, and everything else replaces. Declare a list empty to clear an inherited one.

```json5
plain: { extends: "release", header: [] }
```

## Vars and defines

```json5
vars:   { name: "app", retries: 3 },
define: { DEBUG: false }
```

`name` gives you `{name}` in a path or header and `PCMP_NAME` in your source. `DEBUG` gives you only the constant. Both take a string, a number or a boolean, and the type reaches Luau intact. An integer above 2^53 is [`bad-define`](diagnostics.md#bad-define).

```sh
pcmp build --var version=v1.2.3 --define DEBUG=true
```

!!! warning "What a define can reach"

    `DEBUG`, `_G.DEBUG` and `_G["DEBUG"]` are all substituted.

    `getgenv().DEBUG` is not, because it is a function call. Nothing reports this, and the branch survives into the artifact.

Misspell one and `pcmp check` reports [`unreachable-define`](diagnostics.md#unreachable-define).

## Tokens

```json5
output: "dist/{profile}/{name}.luau"
```

`entry`, `output`, `sources` and `header` take `{token}`. Every var, every axis and `{profile}` expand. Write `{{` and `}}` for a literal brace, and anything else is [`bad-template`](diagnostics.md#bad-template).

A token in a path may not expand to a `.` or `..` segment, so a profile named `..` is refused. A plain `/` is fine, and `{outdir}/app.luau` with `outdir` set to `build/dist` works.

## Entry and output

=== "File to file"

    ```json5
    entry: "src/init.luau", output: "dist/app.luau",
    ```

    Bundled into one file when `darklua.bundle` is set.

=== "Directory to directory"

    ```json5
    entry: "src", output: "build",
    ```

    Every file processed, structure preserved, no bundling.

`header` goes on `.luau` and `.lua`, or whatever `darklua.lua_extension` names.

Two tasks writing one path is [`output-collision`](diagnostics.md#output-collision). A task writing inside an entry tree is [`output-in-inputs`](diagnostics.md#output-in-inputs). An output that climbs out of the project with `..` builds, and `pcmp check` reports [`output-outside-root`](diagnostics.md#output-outside-root).

## Inputs

```json5
sources: ["../shared"],
ignore: ["**/Packages/**"],
```

A build reads every file under the manifest's directory plus every `sources` root, minus anything `ignore` matches. Your outputs and the cache are excluded already. Extension is not a filter, because a loader can make a `.json` or a `.png` a real input.

Requiring a file under no root fails the build with [`undeclared-input`](diagnostics.md#undeclared-input). Add the directory that holds it to `sources`.

## Loaders

```json5
loaders: [
  { pattern: "**/*.png", use: "buffer/base64" },
  { pattern: "**/*.md", use: "string" },
]
```

```lua
local config = require("@self/assets/config.json")
```

| `use` | Behaviour |
| --- | --- |
| `copy` | passed through untouched |
| `skip` | excluded from the output |
| `luau` | parsed and processed as source |
| `json`, `json_lines`, `toml`, `yaml` | returned as parsed data |
| `string`, `buffer`, `bytes` | returned as content |

The content forms also take `/base64`, `/zstd`, `/gzip` or `/zlib`. The first matching pattern wins, so put the specific ones first.

## Axes

```json5
dist: {
  extends: "base",
  output: "dist/{target}/{flavour}.luau",
  axes: {
    flavour: ["min", "dev"],
    target: {
      roblox: { darklua: { bundle: { require_mode: "path" } } },
      lune:   { darklua: { bundle: { require_mode: "luau" } } },
    },
  },
}
```

That is four tasks, named `dist[flavour=min,target=roblox]` and so on. An axis is a list of values, or a map from a value to an overlay that can set any profile field. Each axis is also a var.

```sh
pcmp build dist
pcmp build 'dist[flavour=min,target=roblox]'
pcmp build dist --axis target=roblox
```

## The pcmp API

```lua
pcmp.env("VERSION")               -- errors when unset
pcmp.envOr("VERSION", "v0.0.0")   -- explicit fallback
pcmp.now()                        -- RFC 3339 UTC
pcmp.epoch()                      -- seconds, consistent with now() in one run
pcmp.read("VERSION")              -- a file, relative to the manifest
pcmp.root                         -- the manifest's directory
pcmp.darklua                      -- the linked darklua version
```

These work in `pcmp.luau` and nowhere else. `pcmp` records what they return, so read [Reproducing a build](cli.md#reproducing-a-build) first. There is no `pcmp.exec`, so pass a git SHA in with `--var`.

Manifests run sandboxed. `os`, `io`, `require`, `debug` and `math.random` are all unavailable. `print` goes to stderr.
