---
title: SCIP image release metadata
type: reference
references:
  - target:
      type: task
      id: tsk-61c9
      scope: project
      scope_id: 01KSH1CRFFG8Z74B5ZS78WW808
    relation: implements
---

# SCIP image release metadata

The `release-metadata` Rust executable owns release-version validation, image
discovery, platform selection and tag generation. The GitHub Actions workflow
continues to use upstream Docker actions for metadata/labels, authentication,
QEMU, Buildx, cache and image publication. Running the local producer does not
log in to a registry, build images or publish anything.

## Local invocation

Run from the graphit-scip root, with `images/` as the discovery directory:

```sh
RELEASE_TAG=v0.2.1 cargo run --locked --release --bin release-metadata
```

The executable takes no arguments. `RELEASE_TAG` must be UTF-8; an optional
lowercase `v` is accepted. The version must have exactly three stable SemVer
components, each fitting in `u64`. Zero components are valid. Leading zeros,
prereleases, build metadata, whitespace, overflow and injected newlines fail.
The tag budget is 64 UTF-8 bytes, checked before SemVer parsing. SemVer is
provided by the pinned `semver` crate, without a second handwritten parser.

On success stdout contains one JSON object followed by a newline:

```json
{"patch":"0.2.1","minor":"0.2","major":"0","matrix":{"include":[{"family":"clang","platforms":"linux/amd64"}]},"tags":"type=raw,value=0.2.1\ntype=raw,value=0.2\ntype=raw,value=0\ntype=raw,value=v1\ntype=raw,value=latest"}
```

The example shows one discovered family; the maintained repository currently
contains ten. If `GITHUB_OUTPUT` is set, the producer appends a single-line
`matrix` value and a multiline `tags` value to that file before writing stdout.
Without it, only stdout is written. Errors use exit status 2 and closed messages
on stderr, without echoing invalid tag contents; validation/discovery failures
write neither stdout nor the output file.

## Discovery and resource limits

Discovery streams only immediate children of `images/`. A family is an immediate
directory containing a regular `Dockerfile`. Other regular files and directories
without a regular Dockerfile are ignored; nested image trees are not searched.
Symlinks at the image root, immediate entries or Dockerfiles are rejected. Names
must be nonempty lowercase ASCII letters/digits in segments separated by single
hyphens, with a maximum of 64 bytes. The producer validates each name and the
256-family cap before retaining it, then sorts families by name. This permits
future valid families without a hardcoded production inventory.

| Families | Published platforms |
| --- | --- |
| `clang`, `ruby` | `linux/amd64` |
| All other admitted families | `linux/amd64,linux/arm64` |

The maintained families are clang, dart, dotnet, go, java, php, python, ruby,
rust and typescript. Tags are patch, minor, major, literal `v1` and `latest`.
The literal `v1` is necessary because Graphit Code's `ast.scip.version`
configuration uses it by default; it is independent of the major number.

Each rendered JSON/GitHub payload is capped at 65536 bytes before append/open.
The JSON stdout newline is an additional byte. Buffers reserve their bounded
capacity before serialization, and family retention grows within 256 entries.
These are application budgets, rather than a hard process RSS guarantee or a
limit on kernel directory iteration. Discovery reads metadata only, so source
Dockerfile contents remain unchanged.

Existing output symlinks/nonregular files and physical aliases of discovered
Dockerfiles, including hardlinks, are rejected. Checks use `same-file` and the
standard filesystem API, without platform bindings maintained here. This is a
cooperative filesystem check: there is no claim of atomic protection against
another process replacing paths. Validation, discovery and identity failures
leave existing output unchanged; a final append I/O failure can leave a partial
append, and a later stdout failure can occur after a successful append.

## Workflow consumers and validation

The version job installs the pinned Rust toolchain and runs the locked Cargo
producer with `RELEASE_TAG` from `github.ref_name`. Its `matrix` and `tags`
outputs drive the image job. `fromJSON` supplies an `include` matrix; each row's
`platforms` goes directly to Buildx. Tag instructions go to
`docker/metadata-action`; its labels/tags continue into the upstream build/push
action. The tag trigger, job concurrency, GHCR repository names, publication
permissions, login and cache settings retain their existing behavior.

The workflow installs Rust and compiles a release binary before generating
metadata. The dependency graph is locked; serde derive and semver use their
maintained upstream code.

Run focused checks with:

```sh
cargo test --locked --test release
actionlint .github/workflows/release-images.yml
```

`tests/release.rs` executes the real binary, checks SemVer/family/count/output
boundaries, injection, symlinks, hardlink aliases, unchanged output on failure,
sorted discovery and workflow producer/consumer wiring. The active hook fixture
copies the workflow and maintained image Dockerfiles so these tests run in its
isolated checkout. Validate changed producers and their affected image consumers;
unchanged families retain their existing evidence. Local qualification does not
execute login, release or push steps. Native Windows/macOS execution and real
publication require their corresponding execution before being claimed.

`tests/release.rs` exercises release discovery, tags, semantic versions,
resource bounds and workflow consumers. Generated measurements and execution
logs are temporary outputs, not maintained test fixtures.
