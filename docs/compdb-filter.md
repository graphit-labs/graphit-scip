---
title: C/C++ compilation database filter
type: reference
references:
  - target:
      type: task
      id: tsk-b9c6
      scope: project
      scope_id: 01KSH1CRFFG8Z74B5ZS78WW808
    relation: implements
---

# C/C++ compilation database filter

The clang image runs the Rust `graphit-filter-clang-compdb` executable before
scip-clang when `GRAPHIT_SCIP_HOST_ROOT` is set. It selects allowed translation
units and translates host paths into the container's `/workspace`. The derived
compilation database lives in `/cache`; the host database remains unchanged.
The upstream compiler and scip-clang 0.4.0 are retained. Migration of the shared
entrypoint, other image dispatch logic and release tooling remains separate work.

## Invocation and limits

Build the local executable with `cargo build --locked --release --bin filter-clang-compdb`.
For development, the same Cargo invocation works on Linux, Windows and macOS:

```text
cargo run --locked --bin filter-clang-compdb -- INPUT OUTPUT --allowlist ALLOWED
```

The caller must set `GRAPHIT_SCIP_HOST_ROOT` to its absolute source root.
INPUT and OUTPUT must be different files. Existing symlink and hardlink aliases
are rejected using physical file identity, and identity is checked again before
opening the output. This is a cooperative filesystem check; it does not promise
atomic protection against a hostile process changing paths during that opening.

| Option | Default | Meaning |
| --- | --- | --- |
| `--allowlist FILE` | `/cache/graphit-allowed-files.json` | Optional JSON array of selected relative source names. A missing file disables selection. |
| `--max-input-bytes N` | 134217728 | Maximum bytes retained from INPUT. |
| `--max-allowlist-bytes N` | 33554432 | Maximum bytes retained from the allowlist. |
| `--max-entries N` | 1000000 | Maximum entries in each input array. |
| `--max-output-bytes N` | 134217728 | Maximum serialized output bytes. |

Limits must be positive integers. Host-root metadata has an independent 32768-byte
limit. The input must be a JSON array of objects; the allowlist must be an array
of strings. JSON nesting is limited by the 128-depth validation budget. Unknown
entry fields and non-string values in arguments remain borrowed raw JSON until
serialization, preserving arbitrary-size integers and literal internal-library
marker names without numeric coercion. Their original JSON spelling and internal
whitespace may remain in the output.
Entries and argument arrays keep their original order. Object-key ordering and
Unicode escape spelling are not interfaces.

The reader checks the byte budget before retaining each read chunk. Array visitors
check the entry budget before retaining each entry. Transformed entries are counted
against the serialized output budget before joining the retained array; serialization
finishes within its byte budget before OUTPUT is opened. Windows source-reference
matching bounds each Unicode-folding scratch string at twelve times the output
byte budget before growth; UTF-8 spans are recovered without retaining an offset
array. Canonical source-reference patterns (including `/workspace/`) have a
separate 131072-byte budget. This leaves room for UTF-8 spelling of the
[Windows extended-path length](https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation),
which is measured in Unicode characters rather than UTF-8 bytes;
their folding scratch budget is twelve times that limit. This is independent of
the existing 32768-byte host-root budget. Oversized patterns fail before matcher
construction. Aho-Corasick 1.1.5 uses its contiguous NFA with dense depth zero
and prefilters disabled. Overlapping matches use independent monotonic cursors,
so rejected partial expansions cannot hide valid matches or force repeated
substring scans. The JSON and regular-expression
libraries have temporary allocations; these logical limits are not a hard RSS limit.

## Selection and translation

Linux and macOS host roots use case-sensitive POSIX paths. Windows drive and UNC
roots accept mixed separators. Windows selected-source lookup uses full Unicode
case folding, with the allowlist spelling used for the resulting file, directory
and source references in both `arguments` and `command`. Full folding includes
multi-character expansions such as `ß`/`ss` and `ﬃ`/`ffi`; matching preserves the
original text outside supported source boundaries and cannot match part of an
expanded character. Two different names with the same Windows case-folded key
are rejected; there is no order-dependent choice of canonical spelling.

Absolute paths under the captured host root and existing `/workspace` paths are
translated. Relative source files are resolved against the entry's directory.
A resolvable source excluded by the allowlist is omitted. A source outside the root,
an escaping relative path, or a source that cannot be resolved is retained for
Graphit Code's authoritative final document filter. Empty allowlists therefore
omit resolvable sources while preserving unresolved entries. The manifest alone
is not a sandbox for the upstream indexer.

Embedded roots are rewritten only at supported argument boundaries or joined
compiler flags. Quoted paths, escaped POSIX spaces and Windows include/output
flags retain their command semantics. The filter treats command text as data;
it never executes a shell or a compiler command. It does not claim to implement
an arbitrary shell language.

When unresolved entries remain, stderr reports:

```text
Graphit: retained N unresolved compilation entries for final filtering
```

Success exits zero. Invalid JSON, conflicting names, invalid options, limits,
missing environment and IO failures exit 2 with a closed diagnostic that does not
echo command text, environment contents or paths. Validation/read/serialization
failures leave OUTPUT untouched. A final output-write failure can leave a partial
file; output publication is not atomic. Correct the reported input or budget and
rerun the filter before indexing.

## Verification and resources

`tests/compdb.rs` checks the compilation database with 197 golden cases,
JSON and size limits, Unicode case-folding, command expansion, numeric identity
and depth bounds. `tests/cli.rs` runs the real binary, verifies file preservation
and confirms that command text is not executed. Python is not a test dependency.
The case-folding regression fixture covers `STRASSE.cpp`, selected `Straße.cpp`
and `command=clang -c straße.cpp`; the resulting command uses the source spelling.

Host-format fixtures on Linux do not establish native Windows/macOS behavior.

## Guardrails

The active Git hooks use `core.hooksPath=.githooks`. Install the checked-in hooks
on a new checkout with `git config core.hooksPath .githooks`.

After `cargo fetch --locked`, the offline pre-push check is
`cargo run --locked --offline --target host-tuple --bin xtask -- check`.
Run the focused compile-database contracts with
`cargo test --locked --offline --test compdb --test cli`.

The separate clang qualification uses
`cargo run --locked --offline --bin xtask -- docker-smoke`. It builds the
Linux amd64 image and checks the real clang producer, document selection,
the Unicode Windows command case-fold fixture, and source immutability.
The [standard CI](../.github/workflows/rust.yml) runs the offline contracts on
Linux, Windows and macOS. The [qualification workflow](../.github/workflows/qualification.yml)
runs the image smoke on Linux before a tagged release or on manual dispatch.
