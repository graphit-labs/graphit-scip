use graphit_scip_tools::compdb::{self, FilterError, Limits};
use serde_json::{Value, json, value::RawValue};
use std::collections::BTreeMap;

#[test]
fn golden_cases_and_cross_platform_edges() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/compdb-golden.json")).unwrap();
    assert_eq!(cases.len(), 197);
    for (index, case) in cases.into_iter().enumerate() {
        let source = serde_json::to_vec(&case["commands"]).unwrap();
        let commands = compdb::parse_commands(&source, Limits::default()).unwrap();
        let names = case["allowed"]
            .as_array()
            .map(|v| v.iter().map(|s| s.as_str().unwrap().to_string()).collect());
        let (actual, count) = compdb::transform(
            commands,
            case["root"].as_str().unwrap(),
            names,
            Limits::default(),
        )
        .unwrap();
        let bytes = compdb::serialize(&actual, Limits::default()).unwrap();
        let actual: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(actual, case["output"], "case {index}");
        assert_eq!(
            count,
            case["unresolved"].as_u64().unwrap() as usize,
            "case {index}"
        );
    }
}
#[test]
fn bounds_are_exact_and_one_under_fail() {
    let bytes = br#"[{"file":"/workspace/a.c","unknown":true}]"#;
    let exact = Limits {
        input_bytes: bytes.len(),
        entries: 1,
        ..Limits::default()
    };
    let commands = compdb::parse_commands(bytes, exact).unwrap();
    assert_eq!(
        compdb::parse_commands(
            bytes,
            Limits {
                input_bytes: bytes.len() - 1,
                ..exact
            }
        )
        .unwrap_err(),
        FilterError::Limit
    );
    assert_eq!(
        compdb::parse_commands(br#"[{},{}]"#, exact).unwrap_err(),
        FilterError::Limit
    );
    let output = compdb::serialize(&commands, exact).unwrap();
    assert_eq!(
        compdb::serialize(
            &commands,
            Limits {
                output_bytes: output.len(),
                ..exact
            }
        )
        .unwrap(),
        output
    );
    assert_eq!(
        compdb::serialize(
            &commands,
            Limits {
                output_bytes: output.len() - 1,
                ..exact
            }
        ),
        Err(FilterError::Limit)
    );
    let allow = br#"["a.c"]"#;
    assert_eq!(
        compdb::parse_allowlist(
            allow,
            Limits {
                allowlist_bytes: allow.len(),
                ..exact
            }
        )
        .unwrap(),
        ["a.c"]
    );
    assert_eq!(
        compdb::parse_allowlist(
            allow,
            Limits {
                allowlist_bytes: allow.len() - 1,
                ..exact
            }
        ),
        Err(FilterError::Limit)
    );
    assert_eq!(
        compdb::parse_allowlist(br#"["a","b"]"#, exact),
        Err(FilterError::Limit)
    );
    assert_eq!(
        compdb::parse_commands(
            b"[]",
            Limits {
                entries: 0,
                ..exact
            }
        )
        .unwrap_err(),
        FilterError::InvalidLimits
    );
    let commands = compdb::parse_commands(b"[{},{}]", Limits::default()).unwrap();
    assert_eq!(
        compdb::transform(
            commands,
            "/host",
            None,
            Limits {
                output_bytes: 5,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        FilterError::Limit
    );
}
#[test]
fn malformed_typed_inputs_are_closed() {
    for bytes in [b"null".as_slice(), b"{}", b"[", b"[] trailing"] {
        assert_eq!(
            compdb::parse_commands(bytes, Limits::default()).unwrap_err(),
            FilterError::InvalidJson
        );
    }
    assert_eq!(
        compdb::parse_commands(b"[1]", Limits::default()).unwrap_err(),
        FilterError::InvalidEntry
    );
    for bytes in [b"null".as_slice(), b"{}", b"[1]", b"[] trailing"] {
        assert_eq!(
            compdb::parse_allowlist(bytes, Limits::default()),
            Err(FilterError::InvalidAllowlist)
        );
    }
    let deep = format!(
        "[{{\"unknown\":{}null{}}}]",
        "[".repeat(128),
        "]".repeat(128)
    );
    assert_eq!(
        compdb::parse_commands(deep.as_bytes(), Limits::default()).unwrap_err(),
        FilterError::InvalidJson
    );
    let shallow = format!("[{{\"unknown\":{}null{}}}]", "[".repeat(16), "]".repeat(16));
    assert!(compdb::parse_commands(shallow.as_bytes(), Limits::default()).is_ok());
}
#[test]
fn windows_casefold_is_full_and_ambiguity_is_rejected() {
    let source = serde_json::to_vec(
        &json!([{"directory":r"C:\Work","file":"Straße.cpp","arguments":["clang","Straße.cpp"]}]),
    )
    .unwrap();
    let commands = compdb::parse_commands(&source, Limits::default()).unwrap();
    let (out, count) = compdb::transform(
        commands,
        r"C:\Work",
        Some(vec!["STRASSE.cpp".into()]),
        Limits::default(),
    )
    .unwrap();
    let out: Value =
        serde_json::from_slice(&compdb::serialize(&out, Limits::default()).unwrap()).unwrap();
    assert_eq!(count, 0);
    assert_eq!(out[0]["file"], "/workspace/STRASSE.cpp");
    assert_eq!(out[0]["arguments"][1], "STRASSE.cpp");
    assert_eq!(
        compdb::transform(
            vec![],
            r"C:\Work",
            Some(vec!["a.cpp".into(), "A.cpp".into()]),
            Limits::default()
        )
        .unwrap_err(),
        FilterError::AmbiguousAllowlist
    );
}
#[test]
fn windows_command_source_casefold_expansions_preserve_boundaries_and_text() {
    let regression: Value =
        serde_json::from_str(include_str!("fixtures/compdb-casefold-regression.json")).unwrap();
    let source = serde_json::to_vec(&regression["commands"]).unwrap();
    let limits = Limits::default();
    let commands = compdb::parse_commands(&source, limits).unwrap();
    let (actual, _) = compdb::transform(
        commands,
        regression["root"].as_str().unwrap(),
        Some(vec![regression["allowed"][0].as_str().unwrap().into()]),
        limits,
    )
    .unwrap();
    let actual: Value =
        serde_json::from_slice(&compdb::serialize(&actual, limits).unwrap()).unwrap();
    assert_eq!(actual, regression["expected_output"]);
    assert_ne!(actual, regression["historical_output"]);
    for (original, allowed, command, expected) in [
        (
            "STRASSE.cpp",
            "Straße.cpp",
            r#"clang straße.cpp 'STRASSE.cpp' /workspace/straße.cpp Straße.cpp.bak prefixSTRASSE.cpp -DKEEP=Ö"#,
            r#"clang Straße.cpp 'Straße.cpp' /workspace/Straße.cpp Straße.cpp.bak prefixSTRASSE.cpp -DKEEP=Ö"#,
        ),
        (
            "ffi.cpp",
            "ﬃ.cpp",
            r#"clang 'ﬃ.cpp' ffi.cpp /workspace/FFI.cpp"#,
            r#"clang 'ﬃ.cpp' ﬃ.cpp /workspace/ﬃ.cpp"#,
        ),
        (
            "ss",
            "ß",
            r#"clang 'ß' sß ßs 'ss'"#,
            r#"clang 'ß' sß ßs 'ß'"#,
        ),
        ("s", "S", r#"clang ß s 'ß'"#, r#"clang ß S 'ß'"#),
        ("ss", "SS", "clang ſß", "clang ſSS"),
        (
            "SRC/STRASSE.cpp",
            "src/Straße.cpp",
            r#"clang SRC\straße.cpp /workspace\SRC\STRASSE.cpp"#,
            r#"clang src/Straße.cpp /workspace/src/Straße.cpp"#,
        ),
    ] {
        let source = serde_json::to_vec(&json!([{
            "directory": r"C:\Work",
            "file": original,
            "command": command,
            "foreign": "UNTOUCHED Ö ß ﬃ"
        }]))
        .unwrap();
        let limits = Limits::default();
        let commands = compdb::parse_commands(&source, limits).unwrap();
        let (out, count) =
            compdb::transform(commands, r"C:\Work", Some(vec![allowed.into()]), limits).unwrap();
        let out: Value = serde_json::from_slice(&compdb::serialize(&out, limits).unwrap()).unwrap();
        assert_eq!(count, 0);
        assert_eq!(out[0]["file"], format!("/workspace/{allowed}"));
        assert_eq!(out[0]["command"], expected);
        assert_eq!(out[0]["foreign"], "UNTOUCHED Ö ß ﬃ");
    }
}
#[test]
fn windows_source_pattern_budget_and_overlapping_expansions() {
    let transform = |name: &str, allowed: &str, command: &str| {
        let source = serde_json::to_vec(&json!([{
            "directory": r"C:\Work", "file": name, "command": command
        }]))
        .unwrap();
        let limits = Limits::default();
        let commands = compdb::parse_commands(&source, limits)?;
        let (out, _) = compdb::transform(commands, r"C:\Work", Some(vec![allowed.into()]), limits)?;
        compdb::serialize(&out, limits)
    };
    let exact = "a".repeat(compdb::MAX_SOURCE_PATTERN_BYTES - "/workspace/".len());
    assert!(transform(&exact, &exact, "clang").is_ok());
    let one_under = &exact[..exact.len() - 1];
    assert!(transform(one_under, one_under, "clang").is_ok());
    let one_over = format!("{exact}a");
    assert_eq!(
        transform(&one_over, &one_over, "clang"),
        Err(FilterError::Limit)
    );
    let huge = "a".repeat(compdb::MAX_SOURCE_PATTERN_BYTES * 2);
    assert_eq!(transform(&huge, &huge, "clang"), Err(FilterError::Limit));

    // Every long overlapping match has at least one endpoint inside a ß→ss
    // expansion. Two independent monotonic UTF-8 cursors must reject them.
    let name = "s".repeat(8191);
    let allowed = name.to_uppercase();
    let command = "ß".repeat(65536);
    let output: Value =
        serde_json::from_slice(&transform(&name, &allowed, &command).unwrap()).unwrap();
    assert_eq!(output[0]["command"], command);
    assert_eq!(output[0]["file"], format!("/workspace/{allowed}"));
}
#[test]
fn arbitrary_integers_and_literal_serde_markers_keep_data_identity() {
    let raw=br#"[{"huge":184467440737095516161234567890,"foreign":{"$serde_json::private::Number":"123"},"numeric":{"$serde_json::private::Number":123},"$serde_json::private::Number":"456","array":[{"$serde_json::private::Number":"789"}],"arguments":["clang",{"$serde_json::private::Number":"789"},184467440737095516161234567890]}]"#;
    let limits = Limits::default();
    let commands = compdb::parse_commands(raw, limits).unwrap();
    let (commands, _) = compdb::transform(commands, "/host", None, limits).unwrap();
    let output = compdb::serialize(&commands, limits).unwrap();
    let fields: Vec<BTreeMap<String, &RawValue>> = serde_json::from_slice(&output).unwrap();
    assert_eq!(fields[0]["huge"].get(), "184467440737095516161234567890");
    assert_eq!(
        fields[0]["foreign"].get(),
        r#"{"$serde_json::private::Number":"123"}"#
    );
    assert_eq!(
        fields[0]["numeric"].get(),
        r#"{"$serde_json::private::Number":123}"#
    );
    assert_eq!(fields[0]["$serde_json::private::Number"].get(), r#""456""#);
    assert_eq!(
        fields[0]["array"].get(),
        r#"[{"$serde_json::private::Number":"789"}]"#
    );
    assert_eq!(
        fields[0]["arguments"].get(),
        r#"["clang",{"$serde_json::private::Number":"789"},184467440737095516161234567890]"#
    );
    let leading = br#"[{"$serde_json::private::Number":"123","extra":true}]"#;
    let parsed = compdb::parse_commands(leading, limits).unwrap();
    let output = compdb::serialize(&parsed, limits).unwrap();
    let fields: Vec<BTreeMap<String, &RawValue>> = serde_json::from_slice(&output).unwrap();
    assert_eq!(fields[0]["$serde_json::private::Number"].get(), r#""123""#);
    assert_eq!(fields[0]["extra"].get(), "true");
}
