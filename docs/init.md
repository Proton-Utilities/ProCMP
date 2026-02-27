# pcmp init

The `pcmp init` command scaffolds a complete ProCMP pipeline for your project interactively.

```sh
pcmp init
```

---

## What it generates

```
pipeline/
  .pcmp.json
  .pcmp.local.json
  frames/
    release.luau
    debug.luau
  darklua/
    stable.json
    unminified.json
.env.example
.gitignore
```

---

## Steps

### 1. Project name
Used to name the output files (e.g. `generated/release/my-project.luau`).

### 2. GitHub owner / repo
Optional. If provided, deploy config is added to `.pcmp.json` and deploy prompts are enabled on Release/Beta configs.

### 3. Editor preference
Select your preferred editor. Options:

| Choice | Command |
|--------|---------|
| VS Code | `code --wait {file}` |
| Cursor | `cursor --wait {file}` |
| Neovim | `nvim {file}` |
| Zed | `zed --wait {file}` |
| Skip | — |

### 4. Build configs
Choose which configs to include: Debug, Beta, Release. A directory-based `Tests` config can be added manually later (see [Config Reference](config-reference.md)).

### 5. Entry point
Path to your main Luau source file (e.g. `src/init.luau`). This is the file passed to darklua.

---

## Re-running init

You can run `pcmp init` in an existing project to:

- Regenerate your personal `.pcmp.local.json` (e.g. after switching editors)
- Regenerate frame/darklua templates

!!! warning
    Running `pcmp init` again will overwrite `pipeline/.pcmp.json`. Back it up first if you've customised it.

---

## After init: next steps

1. Copy `.env.example` to `.env` and set your GitHub token
2. Run `pcmp pipeline/.pcmp.json` to build
3. Commit `pipeline/.pcmp.json` and `pipeline/frames/` and `pipeline/darklua/`
4. Add `.pcmp.local.example.json` to show teammates what fields they can set
