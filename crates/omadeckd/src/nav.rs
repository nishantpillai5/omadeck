//! Which page the deck is showing, and the trail of pages that led there.

use omadeck_core::Config;

#[derive(Debug, Default)]
pub struct Nav {
    /// `None` is the home page.
    current: Option<String>,
    history: Vec<Option<String>>,
}

impl Nav {
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    /// Show `page`. Returns whether the deck needs repainting.
    ///
    /// Works like breadcrumbs: going home clears the trail, and going to a page already on
    /// the trail cuts back to it, so keys that link pages in a loop can't grow it forever.
    pub fn go(&mut self, page: Option<&str>) -> bool {
        if self.current.as_deref() == page {
            return false;
        }
        match page {
            None => self.history.clear(),
            Some(p) => match self.history.iter().position(|h| h.as_deref() == Some(p)) {
                Some(pos) => self.history.truncate(pos),
                None => self.history.push(self.current.take()),
            },
        }
        self.current = page.map(str::to_string);
        true
    }

    /// The nearest page on the trail, current first, that `keep` accepts; `None` is Home.
    /// Page steps use it to step from the page a folder hangs off, however deep.
    pub fn nearest(&self, keep: impl Fn(&str) -> bool) -> Option<&str> {
        std::iter::once(&self.current)
            .chain(self.history.iter().rev())
            .map(|p| p.as_deref())
            .find(|p| p.is_none_or(&keep))
            .flatten()
    }

    /// Move sideways to `page` (a flick or page step): replace the current page and clear
    /// the trail, so Back doesn't undo a sideways move.
    pub fn switch(&mut self, page: Option<&str>) -> bool {
        if self.current.as_deref() == page {
            return false;
        }
        self.current = page.map(str::to_string);
        self.history.clear();
        true
    }

    /// Go to the previous page, or home if there is none. Returns whether anything changed.
    pub fn back(&mut self) -> bool {
        if self.current.is_none() {
            return false;
        }
        self.current = self.history.pop().flatten();
        true
    }

    /// Forget pages that no longer exist (after a config reload). Returns whether the
    /// current page changed.
    pub fn retain_existing(&mut self, cfg: &Config) -> bool {
        self.history.retain(|h| h.as_deref().is_none_or(|p| cfg.has_page(p)));
        self.history.dedup();
        match self.current.as_deref() {
            Some(p) if !cfg.has_page(p) => {
                self.current = None;
                self.history.clear();
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omadeck_core::{Keys, Page};

    fn cfg(pages: &[&str]) -> Config {
        let pages = pages.iter().map(|n| Page { name: n.to_string(), keys: Keys::default(), folder: false }).collect();
        Config { pages, ..Config::default() }
    }

    #[test]
    fn folders() {
        let mut n = Nav::default();
        assert!(!n.back(), "back on home does nothing");
        assert!(n.go(Some("A")));
        assert!(n.go(Some("B")));
        assert!(!n.go(Some("B")));
        assert!(n.back());
        assert_eq!(n.current(), Some("A"));
        assert!(n.back());
        assert_eq!(n.current(), None);
    }

    #[test]
    fn loops_stay_bounded() {
        let mut n = Nav::default();
        for _ in 0..10 {
            n.go(Some("A"));
            n.go(Some("B"));
        }
        assert_eq!(n.history, [None, Some("A".to_string())]);
        n.go(None);
        assert!(n.history.is_empty());
    }

    #[test]
    fn switch_clears_the_trail() {
        let mut n = Nav::default();
        n.go(Some("A"));
        n.go(Some("B"));
        assert!(n.switch(Some("C")));
        assert_eq!(n.current(), Some("C"));
        assert!(n.history.is_empty(), "a sideways move is not on the trail");
        assert!(n.back(), "back goes home, not along the old trail");
        assert_eq!(n.current(), None);
        assert!(!n.switch(None), "switching home from home does nothing");
        assert!(n.switch(Some("C")));
        assert!(!n.switch(Some("C")), "switching to the current page does nothing");
    }

    #[test]
    fn nearest_skips_nested_folders() {
        let mut n = Nav::default();
        n.go(Some("Music"));
        n.go(Some("F1"));
        n.go(Some("F2"));
        let not_folder = |p: &str| !p.starts_with('F');
        assert_eq!(n.nearest(not_folder), Some("Music"), "two folders deep steps from Music");
        n.go(None);
        n.go(Some("F1"));
        assert_eq!(n.nearest(not_folder), None, "a folder opened from Home steps from Home");
        n.switch(Some("Music"));
        assert_eq!(n.nearest(not_folder), Some("Music"), "a normal page is its own origin");
    }

    #[test]
    fn deleted_pages() {
        let mut n = Nav::default();
        n.go(Some("A"));
        n.go(Some("B"));
        assert!(!n.retain_existing(&cfg(&["B"])));
        assert_eq!(n.history, [None]);
        assert!(n.retain_existing(&cfg(&[])));
        assert_eq!(n.current(), None);
    }
}
