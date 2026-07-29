//! Content identity.
//!
//! [`Hasher`] length-prefixes everything it takes, so `["ab", "c"]` and `["a", "bc"]`
//! cannot produce the same digest.

use std::collections::BTreeMap;
use std::fmt;

use super::RelPath;

/// A 32-byte blake3 digest. Renders through [`fmt::Display`], so printing one allocates
/// nothing.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Digest([u8; 32]);

/// The first six bytes of a digest, for output a person reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Short(Digest);

impl Digest {
    pub const fn short(self) -> Short {
        Short(self)
    }

    fn write(self, f: &mut fmt::Formatter<'_>, bytes: usize) -> fmt::Result {
        for byte in self.0.iter().take(bytes) {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write(f, 32)
    }
}

impl fmt::Display for Short {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.write(f, 6)
    }
}

impl serde::Serialize for Digest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Digest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;

        let text = <&str as serde::Deserialize>::deserialize(deserializer)?;
        let mut bytes = [0u8; 32];

        if text.len() != 64 {
            return Err(D::Error::custom("a digest is 64 hexadecimal characters"));
        }

        for (index, byte) in bytes.iter_mut().enumerate() {
            let pair = text
                .get(index * 2..index * 2 + 2)
                .ok_or_else(|| D::Error::custom("a digest is 64 hexadecimal characters"))?;
            *byte = u8::from_str_radix(pair, 16).map_err(D::Error::custom)?;
        }

        Ok(Self(bytes))
    }
}

/// Accumulates labelled fields and ordered sequences.
#[derive(Debug, Default)]
pub struct Hasher(blake3::Hasher);

impl Hasher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn field(&mut self, label: &str, bytes: impl AsRef<[u8]>) -> &mut Self {
        let bytes = bytes.as_ref();
        self.0.update(&(label.len() as u64).to_le_bytes());
        self.0.update(label.as_bytes());
        self.0.update(&(bytes.len() as u64).to_le_bytes());
        self.0.update(bytes);
        self
    }

    /// Order-sensitive. The count goes in last, so items can be streamed without
    /// knowing how many there are.
    pub fn seq<I, S>(&mut self, label: &str, items: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<[u8]>,
    {
        let mut count: u64 = 0;
        for (index, item) in items.into_iter().enumerate() {
            let bytes = item.as_ref();
            self.0.update(&(index as u64).to_le_bytes());
            self.0.update(&(bytes.len() as u64).to_le_bytes());
            self.0.update(bytes);
            count += 1;
        }
        self.field(label, count.to_le_bytes())
    }

    pub fn finish(&self) -> Digest {
        Digest(*self.0.finalize().as_bytes())
    }
}

pub fn of(bytes: impl AsRef<[u8]>) -> Digest {
    Digest(*blake3::hash(bytes.as_ref()).as_bytes())
}

/// A digest over a named set of files.
///
/// The reads tier and the artifacts tier are the same question asked of two different
/// sets, so they are the same function given two different labels, which also means the
/// two can never drift into hashing the same bytes differently.
pub fn of_files(label: &str, files: &BTreeMap<RelPath, Vec<u8>>) -> Digest {
    let mut hasher = Hasher::new();
    hasher.seq(
        label,
        files
            .iter()
            .map(|(path, bytes)| format!("{path}\0{}", of(bytes))),
    );
    hasher.finish()
}
