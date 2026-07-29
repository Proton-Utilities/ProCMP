---
description: One Luau source tree, many build targets, from a single manifest.
---

# ProCMP

ProCMP builds one Luau source tree into several artifacts at once. Each artifact comes from a named profile, and every profile lives in one manifest file. [darklua](https://darklua.com) is linked in as a library, so `pcmp` is one binary with no second tool to install.

## Install

=== "rokit"

    ```sh
    rokit add Proton-Utilities/ProCMP
    ```

=== "aftman"

    ```sh
    aftman add Proton-Utilities/ProCMP
    ```

=== "cargo"

    ```sh
    cargo install --locked --git https://github.com/Proton-Utilities/ProCMP
    ```

Add `.pcmp/` to your `.gitignore`.

## Your first build

```console
$ pcmp init
created  pcmp.json5
next     pcmp plan

$ pcmp build --var version=v1.0.0
plan  9b35cd36a0d9

  built   dev      dist/dev/app.luau
  built   release  dist/release/app.luau

2 built, 0 cached, 0 failed

$ pcmp build
0 built, 2 cached, 0 failed
```

## What a profile changes

```lua title="src/init.luau"
--!strict
local VERSION: string = PCMP_VERSION

if DEBUG then
	print("verbose telemetry")
end

return VERSION
```

=== "release"

    ```lua title="dist/release/app.luau"
    -- app v1.0.0
    local a='v1.0.0'return a
    ```

=== "dev"

    ```lua title="dist/dev/app.luau"
    local VERSION: string = 'v1.0.0'

    if true then
        print('verbose telemetry')
    end

    return VERSION
    ```

`release` sets `DEBUG` to `false` and asks darklua to fold and strip. `dev` sets it to `true` and asks for nothing, so a stack trace still points at your source. Both are a few lines of [manifest](manifest.md).

## Editor completion

=== "Data manifest"

    ```sh
    pcmp schema > pcmp.schema.json
    ```

    ```json5 title="pcmp.json5"
    { $schema: "./pcmp.schema.json" }
    ```

=== "Luau manifest"

    ```sh
    pcmp schema --format luau > pcmp.d.luau
    ```

    ```json title=".vscode/settings.json"
    {
      "luau-lsp.types.definitionFiles": ["pcmp.d.luau"]
    }
    ```

Regenerate after an upgrade. If you commit either file, [`stale-schema`](diagnostics.md#stale-schema) tells you when it has gone out of date.
