use serde::Serialize;
use std::{fmt, fs, io::Write, path::Path};

pub const MAX_TAG_BYTES: usize = 64;
pub const MAX_FAMILIES: usize = 256;
pub const MAX_FAMILY_BYTES: usize = 64;
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Tag,
    Images,
    Symlink,
    Family,
    Empty,
    Limit,
    Alias,
    Io,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Tag => "invalid stable release version",
            Self::Images => "cannot discover release images",
            Self::Symlink => "release paths must not be symlinks",
            Self::Family => "invalid image family name",
            Self::Empty => "no release Dockerfiles found",
            Self::Limit => "release metadata limit exceeded",
            Self::Alias => "release output aliases a Dockerfile",
            Self::Io => "cannot write release metadata",
        })
    }
}
impl std::error::Error for Error {}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct Image {
    family: String,
    platforms: &'static str,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
struct Matrix {
    include: Vec<Image>,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Metadata {
    patch: String,
    minor: String,
    major: String,
    matrix: Matrix,
    tags: String,
}

pub fn prepare(tag: &str, images: &Path) -> Result<Metadata, Error> {
    if tag.len() > MAX_TAG_BYTES {
        return Err(Error::Tag);
    }
    let version =
        semver::Version::parse(tag.strip_prefix('v').unwrap_or(tag)).map_err(|_| Error::Tag)?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err(Error::Tag);
    }
    let root = fs::symlink_metadata(images).map_err(|_| Error::Images)?;
    if root.file_type().is_symlink() {
        return Err(Error::Symlink);
    }
    if !root.is_dir() {
        return Err(Error::Images);
    }
    let mut families = Vec::new();
    for entry in fs::read_dir(images).map_err(|_| Error::Images)? {
        let entry = entry.map_err(|_| Error::Images)?;
        let kind = entry.file_type().map_err(|_| Error::Images)?;
        if kind.is_symlink() {
            return Err(Error::Symlink);
        }
        if !kind.is_dir() {
            continue;
        }
        let dockerfile = match fs::symlink_metadata(entry.path().join("Dockerfile")) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(Error::Images),
        };
        if dockerfile.file_type().is_symlink() {
            return Err(Error::Symlink);
        }
        if !dockerfile.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_str().ok_or(Error::Family)?;
        if name.len() > MAX_FAMILY_BYTES
            || name.is_empty()
            || name.split('-').any(|part| {
                part.is_empty()
                    || !part
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            })
        {
            return Err(Error::Family);
        }
        if families.len() == MAX_FAMILIES {
            return Err(Error::Limit);
        }
        if families.len() == families.capacity() {
            let capacity = families.capacity().saturating_mul(2).clamp(4, MAX_FAMILIES);
            families
                .try_reserve_exact(capacity - families.len())
                .map_err(|_| Error::Limit)?;
        }
        families.push(Image {
            family: name.into(),
            platforms: if matches!(name, "clang" | "ruby") {
                "linux/amd64"
            } else {
                "linux/amd64,linux/arm64"
            },
        });
    }
    if families.is_empty() {
        return Err(Error::Empty);
    }
    families.sort_by(|a, b| a.family.cmp(&b.family));
    let patch = version.to_string();
    let minor = format!("{}.{}", version.major, version.minor);
    let major = version.major.to_string();
    let tags = format!(
        "type=raw,value={patch}\ntype=raw,value={minor}\ntype=raw,value={major}\ntype=raw,value=v1\ntype=raw,value=latest"
    );
    Ok(Metadata {
        patch,
        minor,
        major,
        matrix: Matrix { include: families },
        tags,
    })
}

struct Bounded {
    bytes: Vec<u8>,
    limit: usize,
}
impl Bounded {
    fn new(limit: usize) -> Result<Self, Error> {
        if limit == 0 || limit > MAX_OUTPUT_BYTES {
            return Err(Error::Limit);
        }
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(limit).map_err(|_| Error::Limit)?;
        Ok(Self { bytes, limit })
    }
}
impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.limit)
        {
            return Err(std::io::Error::other("limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl Metadata {
    pub fn json(&self, limit: usize) -> Result<Vec<u8>, Error> {
        let mut output = Bounded::new(limit)?;
        serde_json::to_writer(&mut output, self).map_err(|_| Error::Limit)?;
        Ok(output.bytes)
    }
    pub fn github_output(&self, limit: usize) -> Result<Vec<u8>, Error> {
        let mut output = Bounded::new(limit)?;
        output.write_all(b"matrix=").map_err(|_| Error::Limit)?;
        serde_json::to_writer(&mut output, &self.matrix).map_err(|_| Error::Limit)?;
        writeln!(
            output,
            "\ntags<<GRAPHIT_SCIP_RELEASE_TAGS\n{}\nGRAPHIT_SCIP_RELEASE_TAGS",
            self.tags
        )
        .map_err(|_| Error::Limit)?;
        Ok(output.bytes)
    }
    pub fn append(&self, images: &Path, output: &Path) -> Result<(), Error> {
        let rendered = self.github_output(MAX_OUTPUT_BYTES)?;
        match fs::symlink_metadata(output) {
            Ok(file) => {
                if file.file_type().is_symlink() {
                    return Err(Error::Symlink);
                }
                if !file.is_file() {
                    return Err(Error::Io);
                }
                for image in &self.matrix.include {
                    if same_file::is_same_file(
                        output,
                        images.join(&image.family).join("Dockerfile"),
                    )
                    .map_err(|_| Error::Io)?
                    {
                        return Err(Error::Alias);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(Error::Io),
        }
        fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(output)
            .map_err(|_| Error::Io)?
            .write_all(&rendered)
            .map_err(|_| Error::Io)
    }
}
