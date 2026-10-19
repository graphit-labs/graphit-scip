# Position encoding in maintained SCIP producers

`scip-run` completes `Document.position_encoding` before accepting an upstream
index. SCIP's zero value is unspecified; `Metadata.text_document_encoding`
describes file text and does not define the units used by occurrence ranges.

Only an unspecified value is filled, using the producer contracts below.
Coordinates are never converted. An explicit value is preserved, including
future enum values for the consumer to validate. The official SCIP 0.10 and
protobuf 3.7.2 libraries preserve document fields, typed ranges, symbols,
relationships, signatures, documentation and ordinary unknown fields.

| Maintained producer | Contract used for unspecified document encoding | Primary evidence |
| --- | --- | --- |
| Go 0.2.7 | UTF8 code units | [`RangeFromName`](https://github.com/scip-code/scip-go/blob/v0.2.7/internal/symbols/symbols.go#L34) uses Go token byte columns and the byte length of the name. |
| TypeScript 0.4.0 | UTF16 code units | [`Range.ts`](https://github.com/sourcegraph/scip-typescript/blob/v0.4.0/src/Range.ts#L37) uses TypeScript `getLineAndCharacterOfPosition`. |
| Python 0.6.6 | UTF16 code units | [`treeVisitor.ts`](https://github.com/sourcegraph/scip-python/blob/v0.6.6/packages/pyright-scip/src/treeVisitor.ts#L134) uses the Pyright TypeScript text/position utilities, not Python string offsets. |
| Java/Kotlin 0.13.1 | UTF16 code units | Upstream `ScipRange.java`, `ScipVisitor.java` and `LineMap.kt` in [the pinned source](https://github.com/scip-code/scip-java/tree/v0.13.1) use JVM source character offsets and emit typed ranges. |
| Clang 0.4.0 | UTF8 code units | [`Indexer.cc`](https://github.com/sourcegraph/scip-clang/blob/v0.4.0/indexer/Indexer.cc#L55) and `Indexer.h` use Clang presumed byte columns and token byte lengths. |
| .NET 0.2.14-graphit.2 | UTF16 code units; normally explicit already | The maintained [vendor patch](../vendor/scip-dotnet/fix-symbol-origins.patch) emits official field 6 as UTF16; [vendor documentation](../vendor/scip-dotnet/README.md) records Roslyn offsets and the Unicode qualification. |
| Dart 1.6.2 | UTF16 code units | [`utils.dart`](https://github.com/Workiva/scip-dart/blob/1.6.2/lib/src/utils.dart#L35) maps analyzer/Dart string offsets through line information. |
| PHP commit `71a5b117ec4c5dd2af302e363410e604e5df309e` | UTF8 code units; normally explicit already | [`Indexer.php`](https://github.com/davidrjenni/scip-php/blob/71a5b117ec4c5dd2af302e363410e604e5df309e/src/Indexer.php#L75) sets the UTF8 position encoding explicitly. |
| Rust, current rust-analyzer image | No implicit mapping | The producer emits its encoding explicitly. The `rust:1-bookworm` image is not a pinned rust-analyzer source revision, so an unspecified value is rejected. |
| Ruby 0.4.8, x86_64-linux | UTF8 code units | The exact [RubyGems artifact](https://rubygems.org/downloads/scip-ruby-0.4.8-x86_64-linux.gem) was checksum-verified and executed against Unicode definitions and references; its unspecified encoding uses byte columns. See the artifact qualification below. |

The implementation reads and writes one official protobuf message at a time,
without retaining the corpus. A temporary file beside the output replaces it
only after valid EOF and successful flushing. Malformed output, cancellation
or the existing job deadline fail the producer; they do not publish a partially
rewritten index. An unsupported implicit contract is an explicit producer input
error, allowing the product's normal syntax fallback to run.

Focused checks:

```sh
cargo test --locked --offline --lib dispatch::encoding::tests
cargo test --locked --offline --test dispatch
cargo clippy --locked --offline --lib --bin scip-run --bin xtask --test dispatch -- -D warnings
```

The encoding tests use official protobuf round trips with Unicode, typed ranges,
documentation and unknown fields. Dispatch tests execute fixture subprocesses
for all ten families and verify their real output is SCIP. They do not qualify
vendor containers or claim native Windows/macOS, arm64, or published CI results.
The image builders already compile the shared dispatcher from Cargo.lock; new
images must be built and published to distribute this change. No upstream pin,
vendor patch, image release or existing platform qualification is changed here.

On 2026-10-20 the canonical TypeScript Dockerfile produced the local Linux amd64
image `sha256:5a7f55e3bf226b309e7be030859f47f99b04da8e0c848b3c4db26a87d71fedb7`
(`scip-default-local`). Its existing image smoke passed. A separate Unicode
fixture verified explicit UTF16 positions with the official SDK: a definition
after an emoji began at UTF16 column 43, versus byte column 45, and its reference
resolved to the same compiler symbol. Original fixture files remained unchanged.
This qualifies that local producer, not the Graphit App consumer, other image
families, remote publication or other platforms.

The local Rust image
`sha256:842d4e56a995d07388403af544a6a899ed9782d2c46701a725432bc35d973884`
also passed the canonical cold/warm smoke on Linux amd64. Its Dockerfile installs
`rust-src` alongside rust-analyzer: without that component, sysroot discovery
attempted an installation during indexing and stalled on network access. The
image supplies the matching toolchain sources, preserving the existing indexing
command and deadlines. This producer qualification also does not claim the App
integration, remote publication or another platform.

The Ruby 0.4.8 x86_64-linux gem has SHA256
`b8ff1d3c6d40381f53ad09913e18dcf3d2f7c0531727762727f2942654bbfc2d`.
Its executable reports commit `eaa63a4fe7bcd30ca0a0f66e557b60a369eb6ff3`,
based on Sorbet commit `0a7b175bc0bb41e2672369554f74364e83711a84`.
With `😀é` before both a method definition and its reference, it emitted an
unspecified position encoding and ranges `[0,22,26]` and `[1,17,21]`. Their
UTF16 starts would be 19 and 14, and UTF32 starts 18 and 13. Both occurrences
resolve to the same `Object#keep().` symbol, proving byte offsets for this
maintained artifact rather than assuming units from the language.

The small `tests/fixtures/dispatch/ruby-unicode.rb` and `.scip` files preserve
that regression without requiring the vendor executable or network during
tests. Only the fixture metadata project root and tool arguments were made
portable; its vendor documents, occurrences and symbols are unchanged. This
qualifies the exact local native artifact's ranges, not a rebuilt Ruby Docker
image or another platform. Images still need the updated dispatcher before
they gain its Ruby encoding normalization.
