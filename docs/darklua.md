---
description: darklua's own configuration, passed through untouched.
---

# darklua

A profile's `darklua` block goes to darklua unchanged, so anything the linked version supports is available. The full option list is at [darklua.com/docs/config](https://darklua.com/docs/config/).

```json5 title="pcmp.json5"
darklua: {
  generator: { name: "dense", column_span: 120 },
  apply_to_files: ["src/**"],
  skip_files: ["**/*.test.luau"],
  lua_extension: "luau",

  bundle: {
    require_mode: { name: "luau", aliases: { pkg: "./Packages" } },
    excludes: ["@lune/**"],
  },

  rules: [
    "compute_expression",
    { rule: "convert_require", current: "path", target: "roblox" },
  ],
}
```

`apply_to_files` and `skip_files` match a path relative to the entry, so `src/**` matches nothing when the entry is already `src/init.luau`. A filter matching nothing gives you [`no-output`](diagnostics.md#no-output).

## Generators

| Generator | Output |
| --- | --- |
| `retain_lines` | keeps the original line structure, and is darklua's default |
| `dense` | compact, one long line per statement run |
| `readable` | reformatted and indented |

`dense` and `readable` take a `column_span`, either as an object or on their own as a string.

## Rules

| Written | Result |
| --- | --- |
| `rules` omitted | darklua applies its own defaults |
| `rules: []` | bundle and generate, transform nothing |
| `rules: [a, b]` | exactly these, in this order |

In a Luau manifest, `rules = {}` is the empty list. Luau cannot tell an empty list from an
empty map, so `pcmp` reads `{}` as a list at `rules`, `apply_to_files`, `skip_files`,
`excludes` and `globals`. Everywhere else it stays a map.

A rule is a bare name, or an object with a `rule` key and that rule's own settings. Each `define` becomes an `inject_global_value` rule in front of whatever you wrote.

```console
$ pcmp plan release
darklua
  {
    "bundle": { "require_mode": "luau" },
    "generator": "dense",
    "rules": [
      { "rule": "inject_global_value", "identifier": "DEBUG", "value": false },
      { "rule": "inject_global_value", "identifier": "PCMP_NAME", "value": "app" },
      "compute_expression"
    ]
  }
```

`pcmp plan <TASK>` prints the result as a valid `.darklua.json`.

## Rule order

Two orderings are checked, both of them darklua's own.

| Code | What it catches |
| --- | --- |
| [`fold-before-inject`](diagnostics.md#fold-before-inject) | `compute_expression` before `inject_global_value`, so the define does nothing |
| [`branch-before-fold`](diagnostics.md#branch-before-fold) | `remove_unused_if_branch` before `compute_expression`, so the branch survives |

## Merging

| Inherited value | What a child does to it |
| --- | --- |
| object | merges recursively |
| array | replaces outright |
| scalar | replaces |
| `null` | unsets the inherited key |

```json5
unbundled: { extends: "release", darklua: { bundle: null } }
```
