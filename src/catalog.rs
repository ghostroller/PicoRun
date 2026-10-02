use crate::model::AppEntry;
use std::io;

/// Implement Windows discovery behind this boundary. Search owns a stable snapshot.
pub trait AppSource {
    fn discover(&mut self) -> io::Result<Vec<AppEntry>>;
}

#[derive(Debug, Default)]
pub struct Catalog {
    entries: Vec<AppEntry>,
}

impl Catalog {
    pub fn new(entries: Vec<AppEntry>) -> Self {
        Self { entries }
    }

    pub fn entries(&self) -> &[AppEntry] {
        &self.entries
    }

    /// Replace only after a complete successful refresh, preserving the previous snapshot on error.
    pub fn refresh(&mut self, source: &mut impl AppSource) -> io::Result<()> {
        let entries = source.discover()?;
        self.entries = entries;
        Ok(())
    }
}
