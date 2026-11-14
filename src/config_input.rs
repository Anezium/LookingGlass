//! Read-only selection for an absent private configuration file. Supplied bytes
//! and non-NotFound errors never depend on the missing-file policy.
use std::{borrow::Cow, io, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MissingFile {
    OriginalDefaults,
    CompiledRelease,
}

impl MissingFile {
    pub(crate) const fn for_platform() -> Self {
        #[cfg(target_os = "vita")]
        {
            Self::CompiledRelease
        }
        #[cfg(not(target_os = "vita"))]
        {
            Self::OriginalDefaults
        }
    }
}

pub(crate) fn read<'a>(
    path: &Path,
    missing: MissingFile,
    release: &'a [u8],
) -> io::Result<Option<Cow<'a, [u8]>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(Cow::Owned(bytes))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(match missing {
            MissingFile::OriginalDefaults => None,
            MissingFile::CompiledRelease => Some(Cow::Borrowed(release)),
        }),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
pub(crate) struct TestDir(pub(crate) std::path::PathBuf);
#[cfg(test)]
impl TestDir {
    pub(crate) fn new(label: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lg-release-{label}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
#[cfg(test)]
impl Drop for TestDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
