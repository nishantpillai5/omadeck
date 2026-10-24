//! Pages that follow apps (Elgato's Smart Profiles, on pages).

use omadeck_core::Config;

use crate::nav::Nav;

#[derive(Debug, Default)]
pub struct AppFollow {
    /// The page the focused app is linked to, if any.
    link: Option<String>,
    /// Where the deck was before a linked app took it; put back on leaving.
    back: Option<Nav>,
}

impl AppFollow {
    /// Focus moved to an app linked to `link` (`None`: not linked, or nothing focused).
    /// Moves `nav` if it should; returns whether the deck needs repainting.
    pub fn focus(&mut self, link: Option<&str>, nav: &mut Nav, cfg: &Config) -> bool {
        if link == self.link.as_deref() {
            return false;
        }
        // Still where we put it: on the linked page, or in a folder opened from it.
        let ours = self.link.is_some() && nav.nearest(|p| !cfg.is_folder(p)) == self.link.as_deref();
        match link {
            Some(page) => {
                if !ours {
                    self.back = Some(nav.clone());
                }
                self.link = Some(page.to_string());
                nav.switch(Some(page))
            }
            None => {
                self.link = None;
                match self.back.take() {
                    Some(saved) if ours => {
                        let before = nav.clone();
                        *nav = saved;
                        nav.retain_existing(cfg);
                        *nav != before
                    }
                    _ => false,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omadeck_core::{Keys, Page};

    fn cfg() -> Config {
        let page = |name: &str, folder: bool| Page {
            name: name.into(),
            keys: Keys::default(),
            dials: Vec::new(),
            folder,
            apps: Vec::new(),
        };
        let pages = vec![page("Work", false), page("Music", false), page("Chat", false), page("F", true)];
        Config { pages, ..Config::default() }
    }

    #[test]
    fn enter_and_leave_restores_the_folder() {
        let cfg = cfg();
        let (mut f, mut nav) = (AppFollow::default(), Nav::default());
        nav.go(Some("Work"));
        nav.go(Some("F"));
        assert!(f.focus(Some("Music"), &mut nav, &cfg));
        assert_eq!(nav.current(), Some("Music"));
        assert!(!f.focus(Some("Music"), &mut nav, &cfg), "another window of the same app");
        assert!(f.focus(None, &mut nav, &cfg));
        assert_eq!(nav.current(), Some("F"), "back where it was, folder and all");
        assert!(nav.back());
        assert_eq!(nav.current(), Some("Work"), "and the folder's Back still works");
    }

    #[test]
    fn a_then_b_then_leave_returns_to_before_a() {
        let cfg = cfg();
        let (mut f, mut nav) = (AppFollow::default(), Nav::default());
        nav.go(Some("Work"));
        assert!(f.focus(Some("Music"), &mut nav, &cfg));
        assert!(f.focus(Some("Chat"), &mut nav, &cfg));
        assert_eq!(nav.current(), Some("Chat"));
        assert!(f.focus(None, &mut nav, &cfg));
        assert_eq!(nav.current(), Some("Work"));
    }

    #[test]
    fn manual_move_sticks() {
        let cfg = cfg();
        let (mut f, mut nav) = (AppFollow::default(), Nav::default());
        assert!(f.focus(Some("Music"), &mut nav, &cfg));
        nav.switch(Some("Work"));
        assert!(!f.focus(None, &mut nav, &cfg));
        assert_eq!(nav.current(), Some("Work"));
        assert!(f.focus(Some("Music"), &mut nav, &cfg), "focusing it again switches again");
        assert_eq!(nav.current(), Some("Music"));
    }

    #[test]
    fn folder_opened_from_linked_page_still_returns() {
        let cfg = cfg();
        let (mut f, mut nav) = (AppFollow::default(), Nav::default());
        nav.go(Some("Work"));
        assert!(f.focus(Some("Music"), &mut nav, &cfg));
        nav.go(Some("F"));
        assert!(f.focus(None, &mut nav, &cfg));
        assert_eq!(nav.current(), Some("Work"));
    }

    #[test]
    fn manual_move_then_b_saves_new_place() {
        let cfg = cfg();
        let (mut f, mut nav) = (AppFollow::default(), Nav::default());
        nav.go(Some("Work"));
        assert!(f.focus(Some("Music"), &mut nav, &cfg));
        nav.switch(None);
        assert!(f.focus(Some("Chat"), &mut nav, &cfg));
        assert!(f.focus(None, &mut nav, &cfg));
        assert_eq!(nav.current(), None, "back Home, where it was moved by hand");
    }

    #[test]
    fn same_link_is_noop() {
        let cfg = cfg();
        let (mut f, mut nav) = (AppFollow::default(), Nav::default());
        assert!(!f.focus(None, &mut nav, &cfg), "nothing linked, nothing saved");
        assert_eq!(nav.current(), None);
        assert!(f.focus(Some("Music"), &mut nav, &cfg));
        assert!(!f.focus(Some("Music"), &mut nav, &cfg));
        assert_eq!(nav.current(), Some("Music"));
    }
}
