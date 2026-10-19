use super::*;
use protobuf::{EnumOrUnknown, MessageField};
use scip::types::{Index, Occurrence, SingleLineRange, occurrence};
use std::{fs, sync::atomic::Ordering, time::Instant};

fn index(encoding: i32) -> Index {
    let mut metadata = Metadata {
        project_root: "file:///workspace".into(),
        ..Metadata::new()
    };
    metadata
        .special_fields
        .mut_unknown_fields()
        .add_varint(91, 456);
    let mut occurrence = Occurrence {
        symbol: "local 0".into(),
        symbol_roles: 1,
        range: vec![0, 5, 10],
        typed_range: Some(occurrence::Typed_range::SingleLineRange(SingleLineRange {
            line: 0,
            start_character: 5,
            end_character: 10,
            ..SingleLineRange::new()
        })),
        ..Occurrence::new()
    };
    occurrence
        .special_fields
        .mut_unknown_fields()
        .add_length_delimited(92, b"future".to_vec());
    let mut document = Document {
        relative_path: "src/ação.x".into(),
        language: "fixture".into(),
        text: "😀 café".into(),
        position_encoding: EnumOrUnknown::from_i32(encoding),
        occurrences: vec![occurrence],
        symbols: vec![SymbolInformation {
            symbol: "local 0".into(),
            documentation: vec!["Documentação 😀".into()],
            ..SymbolInformation::new()
        }],
        ..Document::new()
    };
    document
        .special_fields
        .mut_unknown_fields()
        .add_varint(93, 789);
    let mut index = Index {
        metadata: MessageField::some(metadata),
        documents: vec![document],
        external_symbols: vec![SymbolInformation {
            symbol: "external".into(),
            documentation: vec!["externo".into()],
            ..SymbolInformation::new()
        }],
        ..Index::new()
    };
    index
        .special_fields
        .mut_unknown_fields()
        .add_length_delimited(94, b"extension".to_vec());
    index
}

#[test]
fn verified_producer_contracts_fill_only_unspecified_encoding() {
    for (family, expected) in [
        (Family::Go, 1),
        (Family::Clang, 1),
        (Family::Php, 1),
        (Family::Ruby, 1),
        (Family::TypeScript, 2),
        (Family::Python, 2),
        (Family::Java, 2),
        (Family::Dotnet, 2),
        (Family::Dart, 2),
    ] {
        let input = index(0);
        let mut output = Vec::new();
        rewrite(
            &mut input.write_to_bytes().unwrap().as_slice(),
            &mut output,
            family,
            &Control::default(),
        )
        .unwrap();
        let actual = Index::parse_from_bytes(&output).unwrap();
        let mut expected_index = input;
        expected_index.documents[0].position_encoding = EnumOrUnknown::from_i32(expected);
        assert_eq!(
            actual, expected_index,
            "{family:?}: only encoding may change"
        );
    }
}

#[test]
fn pinned_ruby_artifact_unicode_ranges_are_utf8_for_definition_and_reference() {
    // Output of scip-ruby 0.4.8 x86_64-linux, gem SHA256 recorded in the docs.
    // Only fixture metadata root/arguments are made portable; vendor documents
    // and their ranges are unchanged. No vendor executable is needed by tests.
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/dispatch/ruby-unicode.scip"
    ));
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/dispatch/ruby-unicode.rb"
    ));
    let original = Index::parse_from_bytes(bytes).unwrap();
    assert_eq!(
        original
            .metadata
            .as_ref()
            .unwrap()
            .tool_info
            .as_ref()
            .unwrap()
            .version,
        "0.4.8"
    );
    let document = &original.documents[0];
    assert_eq!(document.position_encoding.value(), 0);
    let mut symbols = Vec::new();
    for (line, text) in source.lines().enumerate() {
        let byte = text.find("keep").unwrap();
        let utf16 = text[..byte].encode_utf16().count();
        let utf32 = text[..byte].chars().count();
        assert_ne!(byte, utf16);
        assert_ne!(byte, utf32);
        assert_ne!(utf16, utf32);
        let range = [line as i32, byte as i32, (byte + 4) as i32];
        let occurrence = document
            .occurrences
            .iter()
            .find(|o| o.range == range)
            .unwrap();
        assert_eq!(occurrence.symbol_roles & 1, if line == 0 { 1 } else { 0 });
        symbols.push(&occurrence.symbol);
    }
    assert_eq!(symbols[0], symbols[1]);
    let mut output = Vec::new();
    rewrite(
        &mut bytes.as_slice(),
        &mut output,
        Family::Ruby,
        &Control::default(),
    )
    .unwrap();
    let mut expected = original;
    expected.documents[0].position_encoding =
        PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into();
    assert_eq!(Index::parse_from_bytes(&output).unwrap(), expected);
}

#[test]
fn explicit_encodings_are_preserved_for_every_family_including_unknown_enum() {
    for family in [
        Family::Go,
        Family::TypeScript,
        Family::Python,
        Family::Java,
        Family::Clang,
        Family::Dotnet,
        Family::Rust,
        Family::Ruby,
        Family::Dart,
        Family::Php,
    ] {
        for encoding in [1, 2, 3, 123] {
            let input = index(encoding);
            let mut output = Vec::new();
            rewrite(
                &mut input.write_to_bytes().unwrap().as_slice(),
                &mut output,
                family,
                &Control::default(),
            )
            .unwrap();
            assert_eq!(Index::parse_from_bytes(&output).unwrap(), input);
        }
    }
}

#[test]
fn unknown_implicit_contract_and_malformed_output_do_not_replace_original() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("index.scip");
    let input = index(0).write_to_bytes().unwrap();
    fs::write(&path, &input).unwrap();
    assert_eq!(
        normalize(&path, Family::Rust, &Control::default()),
        Err(Error::Input)
    );
    assert_eq!(fs::read(&path).unwrap(), input);
    for input in [
        vec![10, 5, 0],
        vec![18, 2, 10, 0],
        vec![0],
        vec![10, 0, 18, 5, 10],
        vec![10, 0, 43, 52],
    ] {
        fs::write(&path, &input).unwrap();
        assert_eq!(
            normalize(&path, Family::Go, &Control::default()),
            Err(Error::Input)
        );
        assert_eq!(fs::read(&path).unwrap(), input);
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn file_publication_occurs_after_valid_eof_and_preserves_unknown_groups() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("index.scip");
    let mut input = index(0).write_to_bytes().unwrap();
    // Unknown group 5, varint field 6, then matching group terminator.
    input.extend_from_slice(&[43, 48, 7, 44]);
    fs::write(&path, &input).unwrap();
    normalize(&path, Family::Go, &Control::default()).unwrap();
    let output = fs::read(&path).unwrap();
    assert!(output.ends_with(&[43, 48, 7, 44]));
    let mut expected = index(0);
    expected.documents[0].position_encoding =
        PositionEncoding::UTF8CodeUnitOffsetFromLineStart.into();
    assert_eq!(Index::parse_from_bytes(&output).unwrap(), expected);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn cancellation_and_deadline_preserve_output_and_io_checks_cancellation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("index.scip");
    let bytes = index(0).write_to_bytes().unwrap();
    fs::write(&path, &bytes).unwrap();
    let control = Control::default();
    control.cancelled.store(true, Ordering::Relaxed);
    assert_eq!(
        normalize(&path, Family::Go, &control),
        Err(Error::Cancelled)
    );
    let control = Control {
        deadline: Some(Instant::now()),
        ..Control::default()
    };
    assert_eq!(normalize(&path, Family::Go, &control), Err(Error::Deadline));
    assert_eq!(fs::read(&path).unwrap(), bytes);

    struct CancelRead(Control);
    impl Read for CancelRead {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            self.0.cancelled.store(true, Ordering::Relaxed);
            Ok(0)
        }
    }
    let control = Control::default();
    let mut reader = Checked {
        inner: CancelRead(control.clone()),
        control,
    };
    assert!(reader.read(&mut [0]).is_err());
}

#[test]
fn many_documents_are_processed_as_individual_protobuf_messages() {
    let mut bytes = Vec::new();
    let input = index(0);
    {
        let mut output = CodedOutputStream::vec(&mut bytes);
        write_message(&mut output, 1, input.metadata.as_ref().unwrap()).unwrap();
        for n in 0..1000 {
            let mut document = input.documents[0].clone();
            document.relative_path = format!("file{n}.x");
            write_message(&mut output, 2, &document).unwrap();
        }
        output.flush().unwrap();
    }
    let mut output = Vec::new();
    rewrite(
        &mut bytes.as_slice(),
        &mut output,
        Family::Python,
        &Control::default(),
    )
    .unwrap();
    let index = Index::parse_from_bytes(&output).unwrap();
    assert_eq!(index.documents.len(), 1000);
    assert!(
        index
            .documents
            .iter()
            .all(|d| d.position_encoding.value() == 2)
    );
}
