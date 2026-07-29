---
description: Selecting tasks, reading a build, and what an exit code means.
---

# CLI

`pcmp help` lists every command and flag, and `pcmp explain <CODE>` describes any diagnostic you see.

## Selecting tasks

```sh
pcmp build                                    # everything
pcmp build release                            # one profile
pcmp build 'dist[flavour=min,target=roblox]'  # one task
pcmp build dist --axis target=roblox          # a slice
```

`--axis` repeats. There is no wildcard, and a selection matching nothing is [`no-such-task`](diagnostics.md#no-such-task).

## Reading a build

```console
$ pcmp build
plan  9b35cd36a0d9

  built   dev      dist/dev/app.luau
  cached  release  dist/release/app.luau

1 built, 1 cached, 0 failed
```

A task is skipped when its configuration, its inputs and its artifacts are all unchanged. Editing an artifact by hand counts, so the next build restores it.

```console
$ pcmp plan --why
  stale   dev      dist/dev/app.luau      a source file changed
  fresh   release  dist/release/app.luau
```

## Watching

```console
$ pcmp watch
watching  /home/you/app
plan      9b35cd36a0d9

  built   dev  dist/dev/app.luau

1 built, 0 cached, 0 failed
```

Each `sources` root gets a `watching` line of its own.

!!! warning "Editing the manifest does not take effect"

    `watch` reads the manifest once, at startup. An edit there is reported on stderr and otherwise ignored until you restart.

## Reproducing a build

```lua
vars = { version = pcmp.envOr("VERSION", "dev"), built = pcmp.now() }
```

A manifest like this changes between runs. Record what it read, then build from the record.

```sh
pcmp build --lock
pcmp build --frozen    # fails if anything differs
```

A frozen build answers the manifest from `pcmp.lock`, so `pcmp.now()` returns the pinned instant and closes with `2 reproduced, 0 differing`.

Commit `pcmp.lock`. Until it exists, `pcmp check` reports [`unrecorded-reading`](diagnostics.md#unrecorded-reading).

### Pinning the clock

`pcmp.now()` answers differently every second, so a manifest that calls it never hits the cache.

=== "--now"

    ```sh
    pcmp build --now 2026-01-01T00:00:00Z
    ```

=== "SOURCE_DATE_EPOCH"

    ```sh
    SOURCE_DATE_EPOCH=1767225600 pcmp build
    ```

`--now` wins over `SOURCE_DATE_EPOCH`, and a frozen build wins over both.

## Machine-readable output

```sh
pcmp build --json | jq '.tasks[] | select(.status == "failed")'
pcmp check --json | jq '[.[] | select(.severity == "error")] | length'
```

Answers go to stdout and failures go to stderr, so `pcmp build > report.json` still tells you why it did not work. Under `--json` both go to stdout.

??? note "The diagnostic shape"

    ```json
    {
      "code": "missing-output",
      "severity": "error",
      "at": "profiles.release",
      "message": "no `output` after inheritance",
      "help": "a template, e.g. \"dist/{profile}/app.luau\"",
      "source": null
    }
    ```

    `at` is the manifest key, such as `profiles.release.darklua.rules[2]`. `source` carries the operating system's own message when something outside `pcmp` refused.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | success |
| `1` | a task failed, or a `--frozen` build did not reproduce |
| `2` | the command line or the manifest could not be read |
| `3` | `pcmp check` found something |
