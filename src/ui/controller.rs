use crate::{
    catalog::Catalog,
    model::LaunchTarget,
    search::{SearchEngine, SearchHit, MAX_RESULTS},
};

/// Controller state is owned by the window, not borrowed across reentrant Win32 calls.
#[derive(Debug)]
pub struct Controller {
    catalog: Catalog,
    engine: SearchEngine,
    query: String,
    results: Vec<SearchHit>,
    selected: usize,
}

impl Controller {
    pub fn new(catalog: Catalog) -> Self {
        let mut result = Self {
            catalog,
            engine: SearchEngine::default(),
            query: String::new(),
            results: Vec::with_capacity(MAX_RESULTS),
            selected: 0,
        };
        result.recompute();
        result
    }

    /// Return false for notifications whose actual text has not changed.
    pub fn set_query(&mut self, text: &str) -> bool {
        if self.query == text {
            return false;
        }
        self.query.clear();
        self.query.push_str(text);
        self.recompute();
        true
    }

    fn recompute(&mut self) {
        self.engine
            .search(self.catalog.entries(), &self.query, &mut self.results);
        self.selected = 0;
    }

    pub fn select_relative(&mut self, delta: isize) {
        if !self.results.is_empty() {
            self.selected = self
                .selected
                .saturating_add_signed(delta)
                .min(self.results.len() - 1);
        }
    }

    pub fn results(&self) -> &[SearchHit] {
        &self.results
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub fn selected_index(&self) -> Option<usize> {
        (!self.results.is_empty()).then_some(self.selected)
    }

    pub fn selected_target(&self) -> Option<&LaunchTarget> {
        let hit = self.results.get(self.selected)?;
        Some(&self.catalog.entries()[hit.entry_index].target)
    }
}
