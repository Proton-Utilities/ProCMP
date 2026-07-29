---
description: Every code pcmp can report, and what to do about it.
---

# Diagnostics

A code never changes meaning, so it is safe to grep for.

```sh
pcmp explain missing-output
```

## Reading the command line

An unknown flag or an unknown value is rejected separately, with a usage message.

### bad-argument

`error`, exits `2`

An argument's value is not the shape its flag takes. `--env`, `--var`, `--define` and
`--axis` each take `KEY=VALUE`, `--now` takes an RFC 3339 instant in UTC to the second,
and `pcmp explain` takes a code from the list `pcmp explain` prints.

## Reading the manifest

Nothing is built when one of these fires.

### no-manifest

`error`, exits `2`

No manifest in the working directory or any directory above it. Run `pcmp init` to write
one, or point at an existing manifest with `-m`.

### unknown-format

`error`, exits `2`

The extension picks the format, so a JSON manifest called `pcmp.conf` is not read.
Supported: json5, json, jsonc, toml, luau.

### unreadable

`error`, exits `2`

The operating system refused a read. Its own message follows.

### syntax

`error`, exits `2`

The manifest is not valid in the format its extension declares. The parser's own message
follows, with a line and column where it reports one.

### not-a-table

`error`, exits `2`

A Luau manifest must evaluate to a table. The usual cause is a missing `return`.

### eval

`error`, exits `2`

A Luau manifest raised an error while evaluating. Its traceback follows.

### budget

`error`, exits `2`

A Luau manifest exceeded its evaluation budget or its memory limit. The usual cause is a
loop whose condition never becomes false.

### unset-env

`error`, exits `2`

`pcmp.env(name)` was called for a variable that is set neither by `--env` nor in the
process environment. Pass `--env NAME=VALUE`, or use `pcmp.envOr(name, fallback)` when a
default is acceptable.

## Resolving the plan

Every profile is checked before the run gives up, so one edit can fix several at once.

### unknown-base

`error`, exits `2`

`extends` names something that is in neither `templates` nor `profiles`. The help line
lists what is available.

### cyclic-extends

`error`, exits `2`

An `extends` chain returns to a profile it already passed through. The help line lists the
cycle.

### name-collision

`error`, exits `2`

The same name appears in both `templates` and `profiles`. `extends` looks in both, so a
name belongs to one of them. Rename one.

### bad-name

`error`, exits `2`

A profile or template name contains `[`, `]`, `,` or `=`. Those characters delimit a task
identifier such as `dist[target=roblox]`. Rename it.

### missing-entry

`error`, exits `2`

Neither the profile nor anything it extends declares an `entry`, which is a file to
bundle or a directory to process as a tree.

### missing-output

`error`, exits `2`

Neither the profile nor anything it extends declares an `output`, which is a template and
so may vary by profile or axis: "dist/{profile}/app.luau".

### bad-template

`error`, exits `2`

A template names a token that is not a var, not an axis and not `{profile}`, or leaves a
`{` unclosed, or expands to nothing. The help line lists the tokens you can use. Write
`{{` and `}}` for a literal brace.

In a path, a token may not expand to a `.` or `..` segment, so a profile named `..` is
refused. A plain `/` is allowed, and `{outdir}/app.luau` works.

### bad-path

`error`, exits `2`

A path is empty, or climbs above the filesystem root with `..`. Paths resolve against
the manifest's own directory, never the working directory.

### bad-define

`error`, exits `2`

A define key must be a Luau identifier, so `my-flag` and `end` are refused. A value must
be finite, and an integer must survive a round trip through an IEEE double, which bounds
it at 2^53.

### bad-var

`error`, exits `2`

A var name must be a Luau identifier. Two names may also collide, because the constant is
uppercased, so `channel` and `Channel` both give you `PCMP_CHANNEL`.

### bad-rules

`error`, exits `2`

`darklua.rules` is not a list darklua could read. Each entry is a rule name, or an object
with a `rule` key and that rule's own settings.

### bad-loader

`error`, exits `2`

A loader names a strategy darklua does not have. Valid: copy, skip, luau, json,
json_lines, toml, yaml, string, buffer, bytes, and the encoded forms string/base64,
string/zstd, string/gzip and string/zlib, with buffer and bytes likewise.

### bad-loader-pattern

`error`, exits `2`

A loader's `pattern` is not one darklua accepts. A pattern matches a file's path relative
to the entry.

### bad-glob

`error`, exits `2`

An `ignore` entry is not a valid glob. Globs match each file's path relative to the root
it was found under.

### empty-axis

`error`, exits `2`

An axis lists no values, so its profile expands to zero tasks.

### no-tasks

`error`, exits `2`

The manifest declares no profiles, so there is nothing to build. A `templates` entry is
never built on its own.

## Checking the plan

Found once every task is known, so these name two tasks rather than one.

### output-collision

`error`, exits `2`

Two tasks write to the same path, and whichever finished last would win. Give them
distinct `output` templates. `{profile}` and every axis are available as tokens.

### output-in-inputs

`error`, exits `2`

A task writes inside a root that a task reads, so the next build would read the artifact
as a source. Move the output outside every root, or exclude it with `ignore`.

### no-such-task

`error`, exits `2`

No task matched the selection, and the help line lists the ones that exist. A selector is a
profile name or an exact task identifier, and `--axis KEY=VALUE` filters by coordinate.
There is no wildcard.

## Building

Reported per task, and one failing task does not stop the others.

### missing-entry-file

`error`, exits `1`

The task's `entry` does not exist. The path resolves against the manifest's directory, not
the directory you ran from.

### undeclared-input

`error`, exits `1`

darklua asked for a file that is under no root this task declares, and the path it tried
is in the help line. Add the directory holding it to `sources`.

### darklua-config

`error`, exits `1`

darklua rejected the configuration this task compiles to, which is printed with the error.
`pcmp plan <TASK>` shows the same thing without building.

### process-failed

`error`, exits `1`

darklua reported an error while transforming this task's sources. Its own message
follows. When the error is a file darklua could not find, the code is
`undeclared-input` instead.

### no-output

`error`, exits `1`

The task reported no failure and produced nothing. Check `apply_to_files` and `skip_files`,
which match a path relative to the entry, so `src/**` matches nothing when the entry is
already `src/init.luau`.

### write-failed

`error`, exits `1`

An artifact could not be written, and the operating system's own message follows. The
artifact from the previous build is left intact.

### frozen

`error`, exits `1`

A `--frozen` build did not reproduce what `pcmp.lock` records, and the help line names the
tasks that differ. Either the manifest changed since the lock was written, or a task
produced different bytes from the same inputs.

## Lints

Reported by `pcmp check`. `--strict` makes the warnings fail too.

### fold-before-inject

`error`, exits `3`

`compute_expression` is listed before `inject_global_value`, so folding runs first and the
define has no effect. Move every injection ahead of every fold. `pcmp` puts the ones it
generates first, so this only comes from an `inject_global_value` you wrote.

### branch-before-fold

`error`, exits `3`

`remove_unused_if_branch` is scheduled before `compute_expression`. A branch can only be
removed once its condition has folded to a constant, so the branch survives.

### unreachable-define

`warning`, exits `3`

A define's identifier appears in none of the task's sources, so nothing is substituted.
Check the spelling on both sides.

### unrecorded-reading

`warning`, exits `3`

The manifest read the clock or the environment, and no pcmp.lock exists, so nothing
records what it read. Run `pcmp build --lock`, and `pcmp build --frozen` then reproduces
the build exactly, timestamps included.

### shadowed-var

`warning`, exits `3`

A var is named `profile`, which `pcmp` sets itself. The built-in wins. Rename yours.

### output-outside-root

`warning`, exits `3`

A task writes outside the manifest's directory. This builds, and is sometimes what you
want, but `pcmp` is touching files outside the project.

### stale-schema

`warning`, exits `3`

A pcmp.schema.json in the project no longer matches this version of `pcmp`, so your editor
is completing against the wrong thing. Regenerate it with `pcmp schema`, or delete it.

### unused-template

`warning`, exits `3`

Nothing extends this template, and a template is never built, so it does nothing. Remove
it, or move it to `profiles`.

### identical-profiles

`warning`, exits `3`

Two profiles resolve to the same task apart from their output. Move what they share into a
template, or give one of them an axis.

