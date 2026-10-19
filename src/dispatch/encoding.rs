//! Complete the position-encoding contract of the maintained producers.
//! No coordinates are rebased; only an unspecified encoding with a verified
//! producer contract is filled. See docs/scip-position-encoding.md.
use super::{Control, Error, Family};
use protobuf::{CodedInputStream, CodedOutputStream, Message, rt::WireType};
use scip::types::{Document, Metadata, PositionEncoding, SymbolInformation};
use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read, Write},
    path::Path,
};

fn producer_encoding(family: Family) -> Option<PositionEncoding> {
    use PositionEncoding::{
        UTF8CodeUnitOffsetFromLineStart as Utf8, UTF16CodeUnitOffsetFromLineStart as Utf16,
    };
    match family {
        Family::Go | Family::Clang | Family::Php | Family::Ruby => Some(Utf8),
        Family::TypeScript | Family::Python | Family::Java | Family::Dotnet | Family::Dart => {
            Some(Utf16)
        }
        // rust-analyzer emits its encoding explicitly, but the image's upstream
        // revision floats, so no implicit mapping is established for it.
        Family::Rust => None,
    }
}

struct Checked<T> {
    inner: T,
    control: Control,
}
impl<T: Read> Read for Checked<T> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.control.check().map_err(io::Error::other)?;
        let result = self.inner.read(bytes);
        self.control.check().map_err(io::Error::other)?;
        result
    }
}
impl<T: Write> Write for Checked<T> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.control.check().map_err(io::Error::other)?;
        let result = self.inner.write(bytes);
        self.control.check().map_err(io::Error::other)?;
        result
    }
    fn flush(&mut self) -> io::Result<()> {
        self.control.check().map_err(io::Error::other)?;
        self.inner.flush()
    }
}

pub(super) fn normalize(path: &Path, family: Family, control: &Control) -> Result<(), Error> {
    control.check()?;
    let file = File::open(path).map_err(|_| Error::Io)?;
    let mut temporary = tempfile::NamedTempFile::new_in(path.parent().ok_or(Error::Config)?)
        .map_err(|_| Error::Io)?;
    let result = rewrite(
        &mut BufReader::new(file),
        temporary.as_file_mut(),
        family,
        control,
    );
    // Cancellation/deadline remains distinguishable even when observed by an IO
    // adapter inside the protobuf library.
    control.check()?;
    result?;
    temporary.persist(path).map_err(|_| Error::Io)?;
    control.check()
}

fn write_message<M: Message>(
    output: &mut CodedOutputStream<'_>,
    field: u32,
    value: &M,
) -> protobuf::Result<()> {
    output.write_tag(field, WireType::LengthDelimited)?;
    value.write_length_delimited_to(output)
}

fn rewrite(
    reader: &mut dyn Read,
    writer: &mut dyn Write,
    family: Family,
    control: &Control,
) -> Result<(), Error> {
    let mut reader = Checked {
        inner: reader,
        control: control.clone(),
    };
    let mut writer = BufWriter::new(Checked {
        inner: writer,
        control: control.clone(),
    });
    let mut input = CodedInputStream::new(&mut reader);
    let mut output = CodedOutputStream::new(&mut writer);
    let mut metadata_seen = false;
    while let Some(tag) = input.read_raw_tag_or_eof().map_err(|_| Error::Input)? {
        control.check()?;
        match tag {
            10 => {
                let metadata: Metadata = input.read_message().map_err(|_| Error::Input)?;
                metadata_seen = true;
                write_message(&mut output, 1, &metadata).map_err(|_| Error::Io)?;
            }
            18 => {
                let mut document: Document = input.read_message().map_err(|_| Error::Input)?;
                if document.position_encoding.value() == 0 {
                    document.position_encoding =
                        producer_encoding(family).ok_or(Error::Input)?.into();
                }
                write_message(&mut output, 2, &document).map_err(|_| Error::Io)?;
            }
            26 => {
                let symbol: SymbolInformation = input.read_message().map_err(|_| Error::Input)?;
                write_message(&mut output, 3, &symbol).map_err(|_| Error::Io)?;
            }
            _ => copy_unknown(tag, &mut input, &mut output, control)?,
        }
    }
    if !metadata_seen {
        return Err(Error::Input);
    }
    output.flush().map_err(|_| Error::Io)?;
    drop(output);
    writer.flush().map_err(|_| Error::Io)?;
    control.check()
}

// Forward unknown top-level fields rather than constructing an Index retaining
// the entire corpus. Groups are copied iteratively with matching terminators.
fn copy_unknown(
    tag: u32,
    input: &mut CodedInputStream<'_>,
    output: &mut CodedOutputStream<'_>,
    control: &Control,
) -> Result<(), Error> {
    let mut tag = tag;
    let mut groups = Vec::new();
    loop {
        control.check()?;
        let field = tag >> 3;
        if field == 0 {
            return Err(Error::Input);
        }
        output.write_raw_varint32(tag).map_err(|_| Error::Io)?;
        match tag & 7 {
            0 => output
                .write_raw_varint64(input.read_raw_varint64().map_err(|_| Error::Input)?)
                .map_err(|_| Error::Io)?,
            1 => output
                .write_raw_little_endian64(
                    input.read_raw_little_endian64().map_err(|_| Error::Input)?,
                )
                .map_err(|_| Error::Io)?,
            2 => {
                let size = input.read_raw_varint32().map_err(|_| Error::Input)?;
                output.write_raw_varint32(size).map_err(|_| Error::Io)?;
                let mut remaining = size;
                while remaining != 0 {
                    control.check()?;
                    let size = remaining.min(8192);
                    let bytes = input.read_raw_bytes(size).map_err(|_| Error::Input)?;
                    output.write_raw_bytes(&bytes).map_err(|_| Error::Io)?;
                    remaining -= size;
                }
            }
            3 => groups.push(field),
            4 => {
                if groups.pop() != Some(field) {
                    return Err(Error::Input);
                }
            }
            5 => output
                .write_raw_little_endian32(
                    input.read_raw_little_endian32().map_err(|_| Error::Input)?,
                )
                .map_err(|_| Error::Io)?,
            _ => return Err(Error::Input),
        }
        if groups.is_empty() {
            return Ok(());
        }
        tag = input
            .read_raw_tag_or_eof()
            .map_err(|_| Error::Input)?
            .ok_or(Error::Input)?;
    }
}

#[cfg(test)]
mod tests;
