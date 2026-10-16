# Composable checks

Adapters require Reqfile 0.6 or later. Follow the [installation instructions](../README.md#install),
or build this checkout with `cargo install --path . --locked` from the crate root.

A requirement owns one promise. Adapters make its checks reusable without creating
language-specific copies of that promise. The four roles compose:

| Role | Owns | Example |
|---|---|---|
| Requirement | The invariant and its labeled evidence | REPLAY_SAFE |
| Language adapter | Language tooling and report normalization | TypeScript / Vitest, Dart test |
| Library adapter | Library compatibility and relevant scenarios | Effect interruption, Riverpod disposal |
| Project binding | Real operations, fixtures and assertions | Replaying the same invoice command |

These are responsibilities, not four mandatory execution stages. A process requirement
can call a shell command directly. A library adapter can compose a language adapter.
Any language or library can publish an adapter; Reqfile has no framework registry or
universal syntax tree. Commands and decision checks remain the execution leaves.

## Manifest

An adapter directory contains `Adapter.yaml` and the assets its checks need:

```yaml
adapter: 1
kind: language
inputs:
  required: [command, scenarios]
  optional: [cwd]
checks:
  replay-safe:
    - adapter:
        use: ../scenarios
        check: replay-safe
        with: { runner: vitest }
```

Use an export from a requirement's `checks`:

```yaml
- adapter:
    use: ./adapters/typescript
    check: replay-safe
    with:
      command: [npx, --no-install, vitest]
      scenarios:
        same-operation: invoice delivery is replay safe
        distinct-operations: identical invoices with distinct identities both apply
        retry-after-failure: retry resumes the failed invoice
```

Paths resolve relative to the declaring Reqfile or adapter manifest. An explicit Git
`use` is pinned to a full commit, with `path` selecting the adapter directory inside
that repository. Inputs are JSON data exposed to commands as `REQFILE_BINDING`, never
interpolated into shell source. An adapter declares its accepted input names; nested
adapters inherit the inputs they declare, and their explicit `with` values take
precedence. Unknown inputs, missing required inputs, unknown exports, cycles and
unsupported manifest versions are errors.

Command checks compose freely. The current decision service identifies a question by
requirement ID, so an expanded requirement still accepts at most one decision check;
combine its unit selectors rather than silently overwriting another question.

`reqfile update` updates references in Reqfiles. A nested adapter's own pinned
dependencies belong to its author: update those pins explicitly in its manifest
and review the resulting checks before publishing a new adapter version.

Each expanded check retains its own assets and origin. `$REQFILE_ASSETS` points to the
directory of the adapter declaring that leaf, while the command runs in the adopting
Reqfile's directory. Adapter changes invalidate the corresponding changed selection.
`reqfile explain` shows where each composed check came from.

## Project bindings and evidence

The reliability adapters read `.reqfile/<ID>/binding.json` in the checked project. This
consumer-owned data file can accompany an inherited requirement; it does not replace
the shared checks. Its content participates in changed selection. During eval it comes
from the isolated example's `files/`, not the adopting repository.

The [scenario contract](scenarios/DECISIONS.md) specifies required obligations, runner
reports and error behavior. Project tests provide real setup, fault injection and
assertions. The checker verifies that every declared obligation ran successfully and
rejects missing, skipped, duplicate or malformed evidence. It cannot establish that
the assertions are sufficient, that test doubles match production, or that every
operation needing the guarantee was inventoried. Review those separately.

TypeScript and Dart adapters select their test runners. Effect and Riverpod adapters
add library-specific obligations and require a supported installed version. They do
not scan a codebase for a reassuring API name and call the guarantee proved.

## Migration

Existing `command` and `decision` checks keep working. To extract checks, place the
unchanged leaf in a named manifest export and replace it with an `adapter` reference.
Keep its assets beside the manifest. A use block's explicit `checks` still replaces
the whole inherited list; composition is explicit, never an implicit merge.

The nine original requirement packages now expose their existing checks this way.
Their `must`, command options, modes, assets and examples are preserved. In particular,
the eight alpha packages remain alpha and excluded until independently reviewed.
Their historical conventions (such as `core/` and `simulation` names) remain explicit
policy; format migration is not a claim that those conventions suit every repository.

COLOCATION keeps its pinned definition and its exact checker version. Its deterministic
verification and advisory suggestions are separate exports. This preserves the lesson
from its evidence: declared facts can block; inferred ownership requires review.
