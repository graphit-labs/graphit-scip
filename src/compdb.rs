use aho_corasick::{AhoCorasickBuilder, AhoCorasickKind};
use caseless::{Caseless, default_case_fold_str};
use regex::{Regex, RegexBuilder};
use serde::de::{DeserializeSeed, Error as _, SeqAccess, Visitor};

use std::{collections::BTreeMap, fmt, io::Write};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub input_bytes: usize,
    pub allowlist_bytes: usize,
    pub entries: usize,
    pub output_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            input_bytes: 128 * 1024 * 1024,
            allowlist_bytes: 32 * 1024 * 1024,
            entries: 1_000_000,
            output_bytes: 128 * 1024 * 1024,
        }
    }
}
impl Limits {
    pub fn validate(self) -> Result<Self, FilterError> {
        if [
            self.input_bytes,
            self.allowlist_bytes,
            self.entries,
            self.output_bytes,
        ]
        .contains(&0)
        {
            Err(FilterError::InvalidLimits)
        } else {
            Ok(self)
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilterError {
    InvalidLimits,
    InvalidJson,
    InvalidEntry,
    InvalidAllowlist,
    AmbiguousAllowlist,
    Limit,
    InvalidRoot,
}
impl fmt::Display for FilterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLimits => "limits must be positive",
            Self::InvalidJson => "invalid compilation database JSON",
            Self::InvalidEntry => "compilation entries must be objects",
            Self::InvalidAllowlist => "invalid allowed-source manifest",
            Self::AmbiguousAllowlist => "ambiguous Windows allowed-source names",
            Self::Limit => "compilation database limit exceeded",
            Self::InvalidRoot => "host root must be an absolute path",
        })
    }
}
impl std::error::Error for FilterError {}

use serde_json::value::RawValue;
use std::borrow::Cow;

#[derive(Debug)]
pub struct Command<'a> {
    fields: BTreeMap<String, Cow<'a, RawValue>>,
}
impl<'de> serde::Deserialize<'de> for Command<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let fields: BTreeMap<String, &'de RawValue> = serde::Deserialize::deserialize(de)?;
        Ok(Self {
            fields: fields
                .into_iter()
                .map(|(key, value)| (key, Cow::Borrowed(value)))
                .collect(),
        })
    }
}
impl serde::Serialize for Command<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.fields.serialize(serializer)
    }
}
impl Command<'_> {
    fn string(&self, key: &str) -> Option<String> {
        self.fields
            .get(key)
            .and_then(|value| serde_json::from_str::<String>(value.get()).ok())
    }
    fn set_string(&mut self, key: &str, value: &str, limit: usize) -> Result<(), FilterError> {
        let mut output = BoundedBytes::new(limit);
        serde_json::to_writer(&mut output, value).map_err(|_| FilterError::Limit)?;
        self.set_encoded(key, output.into_vec())
    }
    fn set_encoded(&mut self, key: &str, bytes: Vec<u8>) -> Result<(), FilterError> {
        let text = String::from_utf8(bytes).map_err(|_| FilterError::InvalidJson)?;
        let raw = RawValue::from_string(text).map_err(|_| FilterError::InvalidJson)?;
        self.fields.insert(key.into(), Cow::Owned(raw));
        Ok(())
    }
}

// One traversal limits depth without materializing foreign objects or numbers.
// Numeric text and literal serde protocol-marker keys stay ordinary raw JSON.
struct Depth(usize);
impl<'de> DeserializeSeed<'de> for Depth {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Depth {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON with bounded depth")
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        if self.0 >= 128 {
            return Err(A::Error::custom("depth"));
        }
        while map.next_key::<serde::de::IgnoredAny>()?.is_some() {
            map.next_value_seed(Depth(self.0 + 1))?;
        }
        Ok(())
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        if self.0 >= 128 {
            return Err(A::Error::custom("depth"));
        }
        while seq.next_element_seed(Depth(self.0 + 1))?.is_some() {}
        Ok(())
    }
    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<(), E> {
        Ok(())
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<(), E> {
        Ok(())
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<(), E> {
        Ok(())
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<(), E> {
        Ok(())
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
        Ok(())
    }
}
fn push_bounded<T>(values: &mut Vec<T>, value: T, limit: usize) -> Result<(), FilterError> {
    if values.len() >= limit {
        return Err(FilterError::Limit);
    }
    if values.len() == values.capacity() {
        let desired = (values.len() + 1)
            .max(values.capacity().saturating_mul(2))
            .min(limit);
        values
            .try_reserve_exact(desired - values.len())
            .map_err(|_| FilterError::Limit)?;
    }
    values.push(value);
    Ok(())
}

struct Entries(usize);
impl<'de> DeserializeSeed<'de> for Entries {
    type Value = Vec<Command<'de>>;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_seq(self)
    }
}
impl<'de> Visitor<'de> for Entries {
    type Value = Vec<Command<'de>>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a bounded array of objects")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut entries = Vec::new();
        while entries.len() < self.0 {
            let Some(value) = seq.next_element::<Command<'de>>()? else {
                return Ok(entries);
            };
            push_bounded(&mut entries, value, self.0).map_err(|_| A::Error::custom("limit"))?;
        }
        if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
            return Err(A::Error::custom("limit"));
        }
        Ok(entries)
    }
}
pub fn parse_commands(bytes: &[u8], limits: Limits) -> Result<Vec<Command<'_>>, FilterError> {
    limits.validate()?;
    if bytes.len() > limits.input_bytes {
        return Err(FilterError::Limit);
    }
    if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'[') {
        return Err(FilterError::InvalidJson);
    }
    let mut de = serde_json::Deserializer::from_slice(bytes);
    Depth(0)
        .deserialize(&mut de)
        .map_err(|_| FilterError::InvalidJson)?;
    de.end().map_err(|_| FilterError::InvalidJson)?;
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let value = Entries(limits.entries).deserialize(&mut de).map_err(|e| {
        if e.to_string().starts_with("limit") {
            FilterError::Limit
        } else {
            FilterError::InvalidEntry
        }
    })?;
    de.end().map_err(|_| FilterError::InvalidJson)?;
    Ok(value)
}

pub fn parse_allowlist(bytes: &[u8], limits: Limits) -> Result<Vec<String>, FilterError> {
    limits.validate()?;
    if bytes.len() > limits.allowlist_bytes {
        return Err(FilterError::Limit);
    }
    // Check entry count before retaining each manifest string.
    struct Names(usize);
    impl<'de> Visitor<'de> for Names {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a bounded array of strings")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut result = Vec::new();
            while result.len() < self.0 {
                let Some(name) = seq.next_element::<String>()? else {
                    return Ok(result);
                };
                push_bounded(&mut result, name, self.0).map_err(|_| A::Error::custom("limit"))?;
            }
            if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(A::Error::custom("limit"));
            }
            Ok(result)
        }
    }
    use serde::Deserializer as _;
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let result = de.deserialize_seq(Names(limits.entries)).map_err(|e| {
        if e.to_string().starts_with("limit") {
            FilterError::Limit
        } else {
            FilterError::InvalidAllowlist
        }
    })?;
    de.end().map_err(|_| FilterError::InvalidAllowlist)?;
    Ok(result)
}

pub struct BoundedBytes {
    bytes: Vec<u8>,
    limit: usize,
}
impl BoundedBytes {
    pub fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }
    pub fn into_vec(self) -> Vec<u8> {
        self.bytes
    }
}
impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let len = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|n| *n <= self.limit)
            .ok_or_else(|| std::io::Error::other("limit"))?;
        if len > self.bytes.capacity() {
            let capacity = len
                .max(self.bytes.capacity().saturating_mul(2))
                .min(self.limit);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(|_| std::io::Error::other("allocation"))?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub fn serialize(commands: &[Command<'_>], limits: Limits) -> Result<Vec<u8>, FilterError> {
    limits.validate()?;
    let mut writer = BoundedBytes::new(limits.output_bytes);
    serde_json::to_writer(&mut writer, commands).map_err(|_| FilterError::Limit)?;
    Ok(writer.into_vec())
}

fn windows(value: &str) -> bool {
    let b = value.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && matches!(b[2], b'/' | b'\\'))
        || value.starts_with("\\\\")
}
fn normalize(value: &str) -> String {
    let double = value.starts_with("//") && !value.starts_with("///");
    let absolute = value.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in value.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|p| *p != "..") => {
                parts.pop();
            }
            ".." if !absolute => parts.push(part),
            ".." => {}
            _ => parts.push(part),
        }
    }
    let prefix = if double {
        "//"
    } else if absolute {
        "/"
    } else {
        ""
    };
    let result = format!("{prefix}{}", parts.join("/"));
    if result.is_empty() {
        ".".into()
    } else {
        result
    }
}
fn normalize_windows(value: &str) -> String {
    let value = value.replace('\\', "/");
    if value.as_bytes().get(1) == Some(&b':') {
        let rest = normalize(&value[2..]);
        format!("{}{rest}", &value[..2])
    } else {
        normalize(&value)
    }
}
fn workspace_path(value: &str, root: &str, win: bool) -> Option<String> {
    let value_slash = if win {
        value.replace('\\', "/")
    } else {
        value.to_string()
    };
    if value_slash == "/workspace" || value_slash.starts_with("/workspace/") {
        let result = normalize(&value_slash);
        return (result == "/workspace" || result.starts_with("/workspace/")).then_some(result);
    }
    let (root, candidate) = if win {
        if !windows(value) {
            return None;
        }
        (normalize_windows(root), normalize_windows(value))
    } else {
        if !value.starts_with('/') {
            return None;
        }
        (normalize(root), normalize(value))
    };
    let root = root.trim_end_matches('/');
    let same = |a: &str, b: &str| {
        if win {
            a.to_lowercase() == b.to_lowercase()
        } else {
            a == b
        }
    };
    if same(root, &candidate) {
        return Some("/workspace".into());
    }
    // Compare path components, not byte offsets: lowercasing can expand Unicode characters.
    let roots: Vec<&str> = root.split('/').collect();
    let candidates: Vec<&str> = candidate.split('/').collect();
    if roots.len() >= candidates.len() || !roots.iter().zip(&candidates).all(|(a, b)| same(a, b)) {
        return None;
    }
    Some(format!(
        "/workspace/{}",
        candidates[roots.len()..].join("/")
    ))
}
fn source_relative(command: &Command<'_>, root: &str, win: bool) -> Option<String> {
    let source = command.string("file")?;
    let mapped = workspace_path(&source, root, win).or_else(|| {
        if source.starts_with('/') || windows(&source) {
            return None;
        }
        let base = workspace_path(&command.string("directory")?, root, win)?;
        Some(normalize(&format!("{base}/{}", source.replace('\\', "/"))))
    })?;
    mapped.strip_prefix("/workspace/").map(str::to_string)
}

struct Rewriter {
    prefixes: Vec<Regex>,
    joined_flag: Regex,
    win: bool,
    limit: usize,
}
impl Rewriter {
    fn new(root: &str, win: bool, limit: usize) -> Result<Self, FilterError> {
        let mut variants = vec![root.to_string()];
        if win {
            variants.extend([root.replace('\\', "/"), root.replace('/', "\\")]);
        } else {
            variants.push(root.replace(' ', "\\ "));
        }
        variants.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        variants.dedup();
        let prefixes = variants
            .iter()
            .map(|s| {
                RegexBuilder::new(&regex::escape(s))
                    .case_insensitive(win)
                    .size_limit(8 * 1024 * 1024)
                    .build()
                    .map_err(|_| FilterError::InvalidRoot)
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            prefixes,
            joined_flag: Regex::new(r"(?:^|\s)-[A-Za-z][A-Za-z0-9_-]*$").expect("fixed regex"),
            win,
            limit,
        })
    }
    fn rewrite(&self, value: &str, single: bool) -> Result<String, FilterError> {
        let mut value = value.to_string();
        for prefix in &self.prefixes {
            let mut out = String::new();
            let mut cursor = 0;
            for m in prefix.find_iter(&value) {
                if m.start() < cursor {
                    continue;
                }
                let after = value[m.end()..].chars().next();
                if after
                    .is_some_and(|c| !matches!(c, '\\' | '/' | '"' | '\'') && !c.is_whitespace())
                {
                    continue;
                }
                let before = &value[..m.start()];
                let previous = before.chars().next_back();
                if previous.is_some_and(|c| !" \t\n\r\"'=,:".contains(c))
                    && !self.joined_flag.is_match(before)
                {
                    continue;
                }
                let quote = previous.filter(|c| matches!(c, '\'' | '"'));
                let mut end = m.end();
                let mut previous_char = previous;
                for c in value[m.end()..].chars() {
                    if quote.is_some_and(|q| c == q)
                        || (quote.is_none()
                            && !single
                            && (matches!(c, '\'' | '"')
                                || (c.is_whitespace() && previous_char != Some('\\'))))
                    {
                        break;
                    }
                    end += c.len_utf8();
                    previous_char = Some(c);
                }
                append(&mut out, &value[cursor..m.start()], self.limit)?;
                append(&mut out, "/workspace", self.limit)?;
                let suffix = &value[m.end()..end];
                if self.win {
                    for c in suffix.chars() {
                        let mut bytes = [0; 4];
                        append(
                            &mut out,
                            if c == '\\' {
                                "/"
                            } else {
                                c.encode_utf8(&mut bytes)
                            },
                            self.limit,
                        )?;
                    }
                } else {
                    append(&mut out, suffix, self.limit)?;
                }
                cursor = end;
            }
            append(&mut out, &value[cursor..], self.limit)?;
            value = out;
        }
        Ok(value)
    }
}
fn append(output: &mut String, part: &str, limit: usize) -> Result<(), FilterError> {
    if output
        .len()
        .checked_add(part.len())
        .is_none_or(|n| n > limit)
    {
        return Err(FilterError::Limit);
    }
    let len = output.len() + part.len();
    if len > output.capacity() {
        let capacity = len.max(output.capacity().saturating_mul(2)).min(limit);
        output
            .try_reserve_exact(capacity - output.len())
            .map_err(|_| FilterError::Limit)?;
    }
    output.push_str(part);
    Ok(())
}
fn canonicalize(
    value: String,
    original: Option<&str>,
    canonical: Option<&str>,
    win: bool,
    limit: usize,
) -> Result<String, FilterError> {
    let (Some(old), Some(new)) = (original, canonical) else {
        return Ok(value);
    };
    if !win {
        return Ok(value);
    }
    if [old.len(), new.len()]
        .into_iter()
        .any(|n| n > MAX_SOURCE_PATTERN_BYTES - "/workspace/".len())
    {
        return Err(FilterError::Limit);
    }
    let mut value = value;
    for (old, new) in [
        (format!("/workspace/{old}"), format!("/workspace/{new}")),
        (old.to_string(), new.to_string()),
    ] {
        value = replace_windows_source(value, &old, &new, limit)?;
    }
    Ok(value)
}

pub const MAX_SOURCE_PATTERN_BYTES: usize = 128 * 1024;

fn windows_fold(value: &str, limit: usize) -> Result<String, FilterError> {
    // One input scalar can expand to several folded scalars. Bound this scratch
    // buffer before growing it, independently of the retained output buffer.
    let budget = limit.checked_mul(12).ok_or(FilterError::Limit)?;
    let mut folded = String::new();
    for c in value
        .chars()
        .map(|c| if c == '\\' { '/' } else { c })
        .default_case_fold()
    {
        let mut bytes = [0; 4];
        append(&mut folded, c.encode_utf8(&mut bytes), budget)?;
    }
    Ok(folded)
}

#[derive(Clone)]
struct FoldPosition<'a> {
    chars: std::str::CharIndices<'a>,
    folded: usize,
    original: usize,
}
impl FoldPosition<'_> {
    fn advance(&mut self, target: usize) -> Option<usize> {
        while self.folded < target {
            let (index, c) = self.chars.next()?;
            self.original = index + c.len_utf8();
            self.folded += std::iter::once(if c == '\\' { '/' } else { c })
                .default_case_fold()
                .map(char::len_utf8)
                .sum::<usize>();
        }
        (self.folded == target).then_some(self.original)
    }
}

fn replace_windows_source(
    value: String,
    old: &str,
    new: &str,
    limit: usize,
) -> Result<String, FilterError> {
    let folded = windows_fold(&value, limit)?;
    if old.len() > MAX_SOURCE_PATTERN_BYTES {
        return Err(FilterError::Limit);
    }
    let needle = windows_fold(old, MAX_SOURCE_PATTERN_BYTES)?;
    if folded == needle {
        let mut result = String::new();
        append(&mut result, new, limit)?;
        return Ok(result);
    }
    if needle.is_empty() || !folded.contains(&needle) {
        if value.len() > limit {
            return Err(FilterError::Limit);
        }
        return Ok(value);
    }
    let matcher = AhoCorasickBuilder::new()
        .kind(Some(AhoCorasickKind::ContiguousNFA))
        .dense_depth(0)
        .prefilter(false)
        .build([&needle])
        .map_err(|_| FilterError::Limit)?;
    let mut start_position = FoldPosition {
        chars: value.char_indices(),
        folded: 0,
        original: 0,
    };
    let mut end_position = start_position.clone();
    let mut cursor = 0;
    let mut result = String::new();
    for found in matcher.find_overlapping_iter(&folded) {
        let original_start = start_position.advance(found.start());
        let original_end = end_position.advance(found.end());
        if let (Some(from), Some(to)) = (original_start, original_end) {
            if from < cursor {
                continue;
            }
            let previous = value[..from].chars().next_back();
            let next = value[to..].chars().next();
            if !previous.is_some_and(|c| c.is_ascii_alphanumeric() || "_./\\".contains(c))
                && !next.is_some_and(|c| !c.is_whitespace() && !matches!(c, '\'' | '"'))
            {
                append(&mut result, &value[cursor..from], limit)?;
                append(&mut result, new, limit)?;
                cursor = to;
            }
        }
    }
    append(&mut result, &value[cursor..], limit)?;
    Ok(result)
}
const FLAGS: [&str; 9] = [
    "-include",
    "-idirafter",
    "-isystem",
    "-iquote",
    "-imacros",
    "-iframework",
    "-I",
    "-F",
    "-o",
];
fn normalize_argument(value: String) -> String {
    if !value.contains('\\') || windows(&value) {
        return value;
    }
    for flag in FLAGS {
        if let Some(rest) = value.strip_prefix(flag).filter(|s| !s.is_empty()) {
            return format!("{flag}{}", rest.replace('\\', "/"));
        }
    }
    if !value.starts_with('-') {
        value.replace('\\', "/")
    } else {
        value
    }
}
fn normalize_command(value: String) -> String {
    let pattern =
        Regex::new(r#"-(?:include|idirafter|isystem|iquote|imacros|iframework|I|F|o)[^\s"']+"#)
            .expect("fixed regex");
    let mut output = String::new();
    let mut cursor = 0;
    for m in pattern.find_iter(&value) {
        if value[..m.start()]
            .chars()
            .next_back()
            .is_some_and(|c| !c.is_whitespace())
        {
            continue;
        }
        output.push_str(&value[cursor..m.start()]);
        output.push_str(&m.as_str().replace('\\', "/"));
        cursor = m.end();
    }
    output.push_str(&value[cursor..]);
    output
}

fn rewrite_arguments<F: FnMut(&RawValue) -> Result<Option<String>, FilterError>>(
    raw: &RawValue,
    limit: usize,
    mut change: F,
) -> Result<Vec<u8>, FilterError> {
    struct Arguments<'a, F> {
        output: BoundedBytes,
        change: &'a mut F,
    }
    impl<'de, F: FnMut(&RawValue) -> Result<Option<String>, FilterError>> Visitor<'de>
        for Arguments<'_, F>
    {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an argument array")
        }
        fn visit_seq<A: SeqAccess<'de>>(mut self, mut seq: A) -> Result<Vec<u8>, A::Error> {
            self.output.write_all(b"[").map_err(A::Error::custom)?;
            let mut first = true;
            while let Some(value) = seq.next_element::<&RawValue>()? {
                if !first {
                    self.output.write_all(b",").map_err(A::Error::custom)?;
                }
                first = false;
                if let Some(value) = (self.change)(value).map_err(A::Error::custom)? {
                    serde_json::to_writer(&mut self.output, &value).map_err(A::Error::custom)?;
                } else {
                    self.output
                        .write_all(value.get().as_bytes())
                        .map_err(A::Error::custom)?;
                }
            }
            self.output.write_all(b"]").map_err(A::Error::custom)?;
            Ok(self.output.into_vec())
        }
    }
    use serde::Deserializer as _;
    let mut de = serde_json::Deserializer::from_str(raw.get());
    let output = de
        .deserialize_seq(Arguments {
            output: BoundedBytes::new(limit),
            change: &mut change,
        })
        .map_err(|_| FilterError::Limit)?;
    de.end().map_err(|_| FilterError::InvalidJson)?;
    Ok(output)
}

pub fn transform<'a>(
    commands: Vec<Command<'a>>,
    root: &str,
    allowed: Option<Vec<String>>,
    limits: Limits,
) -> Result<(Vec<Command<'a>>, usize), FilterError> {
    limits.validate()?;
    if commands.len() > limits.entries || root.len() > 32 * 1024 {
        return Err(FilterError::Limit);
    }
    let win = windows(root);
    if root.is_empty() || (!win && !root.starts_with('/')) {
        return Err(FilterError::InvalidRoot);
    }
    let allowed = allowed
        .map(|names| {
            if names.len() > limits.entries {
                return Err(FilterError::Limit);
            }
            let mut lookup = BTreeMap::new();
            for name in names {
                if name.len() > limits.allowlist_bytes {
                    return Err(FilterError::Limit);
                }
                let key = if win {
                    default_case_fold_str(&name)
                } else {
                    name.clone()
                };
                if lookup.get(&key).is_some_and(|old| old != &name) {
                    return Err(FilterError::AmbiguousAllowlist);
                }
                lookup.insert(key, name);
            }
            Ok(lookup)
        })
        .transpose()?;
    let rewrite = Rewriter::new(root, win, limits.output_bytes)?;
    struct Count {
        bytes: usize,
        limit: usize,
    }
    impl Write for Count {
        fn write(&mut self, part: &[u8]) -> std::io::Result<usize> {
            self.bytes = self
                .bytes
                .checked_add(part.len())
                .filter(|n| *n <= self.limit)
                .ok_or_else(|| std::io::Error::other("limit"))?;
            Ok(part.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    if limits.output_bytes < 2 {
        return Err(FilterError::Limit);
    }
    let mut output_budget = Count {
        bytes: 2,
        limit: limits.output_bytes,
    };
    let mut retained = Vec::new();
    let mut unresolved = 0;
    for mut command in commands {
        let original = source_relative(&command, root, win);
        let mut rel = original.clone();
        if let Some(lookup) = &allowed {
            if let Some(ref r) = rel {
                let key = if win {
                    default_case_fold_str(r)
                } else {
                    r.clone()
                };
                let Some(canonical) = lookup.get(&key) else {
                    continue;
                };
                rel = Some(canonical.clone());
            } else {
                unresolved += 1;
            }
        }
        let mut relative = None;
        if win
            && let (Some(old), Some(new), Some(directory)) = (
                &original,
                &rel,
                command
                    .string("directory")
                    .and_then(|s| workspace_path(&s, root, win)),
            )
        {
            let dir: Vec<&str> = directory
                .strip_prefix("/workspace/")
                .unwrap_or("")
                .split('/')
                .filter(|s| !s.is_empty())
                .collect();
            let old_parts: Vec<&str> = old.split('/').collect();
            let new_parts: Vec<&str> = new.split('/').collect();
            if dir.len() <= old_parts.len()
                && dir
                    .iter()
                    .zip(&old_parts)
                    .all(|(a, b)| default_case_fold_str(a) == default_case_fold_str(b))
            {
                let prefix = new_parts[..dir.len().min(new_parts.len())].join("/");
                command.set_string(
                    "directory",
                    &if prefix.is_empty() {
                        "/workspace".into()
                    } else {
                        format!("/workspace/{prefix}")
                    },
                    limits.output_bytes,
                )?;
                relative = Some((
                    old_parts[dir.len()..].join("/"),
                    new_parts[dir.len().min(new_parts.len())..].join("/"),
                ));
            }
        }
        for key in ["directory", "file"] {
            if key == "file"
                && let Some(r) = &rel
            {
                command.set_string(key, &format!("/workspace/{r}"), limits.output_bytes)?;
                continue;
            }
            if let Some(value) = command.string(key) {
                let value = workspace_path(&value, root, win)
                    .map(Ok)
                    .unwrap_or_else(|| rewrite.rewrite(&value, true))?;
                command.set_string(key, &value, limits.output_bytes)?;
            }
        }
        if let Some(value) = command.string("command") {
            let value_new = rewrite.rewrite(&value, false)?;
            let value_new = canonicalize(
                value_new,
                original.as_deref(),
                rel.as_deref(),
                win,
                limits.output_bytes,
            )?;
            let value_new = canonicalize(
                value_new,
                relative.as_ref().map(|r| r.0.as_str()),
                relative.as_ref().map(|r| r.1.as_str()),
                win,
                limits.output_bytes,
            )?;
            command.set_string(
                "command",
                &if win {
                    normalize_command(value_new)
                } else {
                    value_new
                },
                limits.output_bytes,
            )?;
        }
        if let Some(raw) = command
            .fields
            .get("arguments")
            .filter(|v| v.get().starts_with('['))
        {
            let encoded = rewrite_arguments(raw, limits.output_bytes, |raw| {
                let Ok(value) = serde_json::from_str::<String>(raw.get()) else {
                    return Ok(None);
                };
                let value_new = rewrite.rewrite(&value, true)?;
                let value_new = canonicalize(
                    value_new,
                    original.as_deref(),
                    rel.as_deref(),
                    win,
                    limits.output_bytes,
                )?;
                let value_new = canonicalize(
                    value_new,
                    relative.as_ref().map(|r| r.0.as_str()),
                    relative.as_ref().map(|r| r.1.as_str()),
                    win,
                    limits.output_bytes,
                )?;
                Ok(Some(if win {
                    normalize_argument(value_new)
                } else {
                    value_new
                }))
            })?;
            command.set_encoded("arguments", encoded)?;
        }
        if !retained.is_empty() {
            output_budget
                .write_all(b",")
                .map_err(|_| FilterError::Limit)?;
        }
        serde_json::to_writer(&mut output_budget, &command).map_err(|_| FilterError::Limit)?;
        push_bounded(&mut retained, command, limits.entries)?;
    }
    Ok((retained, unresolved))
}
