---
title: Corrected .NET SCIP producer
type: reference
references:
  - target:
      type: task
      id: tsk-4e14
      scope: project
      scope_id: 01KSH1CRFFG8Z74B5ZS78WW808
    relation: implements
---

# Corrected .NET SCIP producer

The .NET image builds the pinned external scip-dotnet source with the local
correction described in [the vendor receipt](../vendor/scip-dotnet/README.md).
The own dispatcher and qualification tools remain Rust. Its preserved CLI
contract is `scip-dotnet index`, with genuine Roslyn symbol information and no
SCIP normalization after emission.

The dispatcher discovers solutions and C#/Visual Basic projects recursively,
including repositories whose projects are all below the workspace root. It
does not follow directory or file symlinks, enter hidden directories, or scan
generated `bin`, `obj`, `node_modules` and `target` trees. Inputs are ordered
deterministically by solution format, project format and full relative path.
For solutions, the SDK's `dotnet sln <solution> list` identifies covered projects;
those projects are not also passed as standalone inputs. Both `.sln` and `.slnx`
use that SDK operation. Listing and restore failures are propagated.

Only the selected solutions and remaining standalone projects are restored.
One `scip-dotnet index --output <index> --skip-dotnet-restore <inputs>...`
invocation accumulates their documents into one index. Its working directory
remains the workspace root, so document paths retain the original repository
layout. A workspace with no solution or project fails explicitly instead of
invoking the indexer's current-directory autodetection.

The qualified package is 0.2.14-graphit.2/net10 with
coherent Roslyn 5.9 and SDK 10.0.401 for both build and runtime. Private full
workspace probes pass all four original/modern C# and Visual Basic projects,
both fresh and prebuilt. Source-built Roslyn Task/BuildHost transports the
compiler's original argument bytes through explicit item metadata; the reader
fails when metadata is absent. Public workspace properties select the packaged
Task/targets together. The VB escaped-constant fixture follows the branch
observed with the stock SDK, while preserving backslash, percent, semicolon and
doubled-quote input. No inverse string repair or property fallback is used.
C#14 contextual `field` references retain their actual implicit-field symbol,
associated-property relationship and reference-only keyword range. The qualified image passes accessible-source decode/lint/stats for all four
original/modern C#/VB fixtures in cold and warm runs.

## Corpus and source navigation

`tests/fixtures/vendor-dotnet` contains real projects for global/top-level
C#, block/file-scoped namespaces, A.B/C.B identities, repeated and partial
namespaces, Unicode identifiers, a supplementary-plane character before a
reference, source/external aliases, BCL and Newtonsoft.Json 13.0.3 references,
source+BCL merged namespaces, implicit constructors, accessors, records,
different generic arities, interface implementations and overrides. VB projects
include `RootNamespace` and partial namespaces. New net10 fixtures exercise C#14 field-backed properties and VB custom/default
conditional constants. SDK-generated documents are
prepared and retained so their emitted positions can be checked against actual
source.

The Rust verifier checks declaration tokens and roles, own-document symbol
information, distinct IDs, alias targets, implicit declaring relationships,
source versus external namespace information and real concrete assembly owners.
Every emitted range is validated as UTF-16 code units against source, including
generated documents; the emoji counterproof rejects UTF-8 columns. Official
SCIP print/strict lint/stats are required, together with the real tool version
and document position encoding. This does not claim Razor, arbitrary source
generators, all .NET frameworks or native non-Linux platforms.

Failure fixtures distinguish invalid project/solution, unavailable dependencies,
compiler errors and MSBuild workspace failure. Logs retain both inconclusive
initial restore failures and specific failure diagnostics. A network-disabled empty
local feed proves that local package installation cannot silently fetch a
fallback tool. No successful index is allowed for these failure cases.

## Build and validation

```console
cargo run --locked --bin xtask -- check
cargo run --locked --bin xtask -- vendor-dotnet-smoke
cargo run --locked --bin xtask -- vendor-dotnet-corpus sha256:3552d2cc885d785e1d3d39454a4c47ac700fae2c40a00155588d46bb0ea36f9a
cargo run --locked --bin xtask -- images-smoke dotnet sha256:3552d2cc885d785e1d3d39454a4c47ac700fae2c40a00155588d46bb0ea36f9a
```

The smoke command builds the image with Docker's normal cache and exercises
four real C#/VB projects through the Rust dispatcher. `vendor-dotnet-corpus`
takes an existing `sha256:imageID` and runs the same tests without building.
`images-smoke dotnet` provides the same focused image test.

Each project runs cold and warm. The official `scip` CLI decodes and lints the
index; tests also check a source document and symbol ranges, cache reuse, source
preservation, invalid-family rejection and deadline cleanup. Restoration uses
NuGet normally and may require network access; restoration has a two-minute
timeout. Integration requires Linux
amd64 Docker with OverlayFS mount permission and `scip` on PATH.

Test directories are disposable, containers use `--rm`, and build output streams
to the terminal. Qualification does not depend on old image receipts or packaged
SDK diagnostics. The Docker recipe retains source/patch checksums and its normal
package build, local installation and attribution checks.

The package preserves its Apache `LICENSE` and root `NOTICE`, plus
`GraphitRoslyn.LICENSE` and `GraphitRoslyn.NOTICE` from pinned source. Docker copies
both projects' license/notice files under `/opt/vendor-provenance`. Package and
runtime verification check delivered attribution bytes against source.

## Maintained build inputs

`vendor/scip-dotnet/upstream.json` identifies the pinned source, patch, protocol
generator, SDK and package. `images/dotnet/Dockerfile` builds these inputs.
Tests under `tests/fixtures/vendor-dotnet` contain the original and modern C#/VB
projects; generated indexes and logs are not source dependencies.

Source-built Roslyn uses `NUGET_PACKAGES=/nuget` for Arcade localization tasks.
Compiler inner-build queries select `TargetFramework=net10.0`. Compiler argument
transport preserves the SDK's original explicit item metadata and fails when it
is absent. Windows/macOS, Docker Desktop, arm64 and net472 execution remain
separate integration validations.
