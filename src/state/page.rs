//! The page table: what each [`Page`] is called, which key opens it and how it draws.
//!
//! Every dispatcher reads this table instead of matching on the variant itself — `ui::draw`
//! takes the drawing entry points from it, the main key map asks it which page a key opens,
//! and the help popup writes its page rows from it. Adding a page is a variant on [`Page`],
//! a row here and the page's own drawing — nothing else has to learn about the page. A page
//! that is *about* something rather than about the app (an artist, say) gets `key: None` and
//! is opened by whatever knows what it is about: the row that names it.

use ratatui::{Frame, layout::Rect};

use crate::{
    app::App,
    config::{NavPosition, PanesConfig},
    layout::{self, LayoutAreas},
    ui,
};

/// Top-level screens the TUI can be on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Splash,
    Main,
    Lyrics,
    Playlist,
    /// One artist's profile, hot songs and albums. It is about a single row of the
    /// hot-artists table rather than about the app, which is why it has no key: see
    /// [`Page::spec`].
    Artist,
    Login,
}

/// What the dispatchers need to know about one page, decided once.
pub struct PageSpec {
    /// Chinese name, as the help key map writes it.
    pub name: &'static str,
    /// The key that opens the page — and, pressed on the page itself, leaves it again.
    /// `None` when no key names the page: it is opened and left by what it is about.
    pub key: Option<char>,
    /// How the page draws.
    pub render: PageRender,
}

/// How a page draws: the whole frame, or its own area inside the shared shell.
#[derive(Clone, Copy)]
pub enum PageRender {
    /// The page owns every cell of the frame; there is no topbar and no player bar.
    Standalone(fn(&mut Frame, &mut App, Rect)),
    /// The page draws inside the shared shell: `ui::draw` runs `layout` and paints the
    /// topbar and player bar, then hands the page's own area to `content`.
    Shell {
        layout: fn(Rect, &PanesConfig, NavPosition) -> LayoutAreas,
        content: fn(&mut Frame, &mut App, &LayoutAreas),
    },
}

impl Page {
    /// Every page, in table order.
    pub const ALL: [Page; 6] = [
        Page::Splash,
        Page::Main,
        Page::Lyrics,
        Page::Playlist,
        Page::Artist,
        Page::Login,
    ];

    /// The page's row in the table.
    pub fn spec(self) -> &'static PageSpec {
        match self {
            Page::Splash => &PageSpec {
                name: "启动页",
                key: None,
                render: PageRender::Standalone(ui::draw_splash),
            },
            Page::Main => &PageSpec {
                name: "主界面",
                key: None,
                render: PageRender::Shell {
                    layout: layout::main,
                    content: ui::draw_main,
                },
            },
            Page::Lyrics => &PageSpec {
                name: "歌词页 / 主界面",
                key: Some('l'),
                render: PageRender::Shell {
                    layout: layout::content,
                    content: ui::draw_lyrics,
                },
            },
            Page::Playlist => &PageSpec {
                name: "播放队列 / 主界面",
                key: Some('f'),
                render: PageRender::Shell {
                    layout: layout::content,
                    content: ui::draw_queue,
                },
            },
            Page::Artist => &PageSpec {
                name: "歌手详情 / 主界面",
                // No key on purpose: the page is about one artist, so it is opened from that
                // artist's row in the hot-artists table (Enter), not from anywhere in the app.
                // Its entry and exit keys live with that row, in `input::main`, and Esc is
                // what leaves the page — the table's `key` column has no way to name an
                // artist, so a key here would open a page with nothing in it.
                key: None,
                render: PageRender::Shell {
                    layout: layout::content,
                    content: ui::draw_artist,
                },
            },
            Page::Login => &PageSpec {
                name: "登录网易云",
                key: Some('L'),
                render: PageRender::Standalone(ui::draw_login),
            },
        }
    }

    /// The page a key opens, read from the table's `key` column.
    pub fn opened_by(key: char) -> Option<Page> {
        Page::ALL
            .into_iter()
            .find(|page| page.spec().key == Some(key))
    }

    /// Where a page key leads from `self`: the main page opens the page the key names, that
    /// page's own key comes back to the main page, and so does any other page's key. The
    /// splash ignores keys.
    pub fn on_key(self, key: char) -> Option<Page> {
        let opened = Page::opened_by(key)?;
        Some(match self {
            Page::Splash => Page::Splash,
            Page::Main => opened,
            _ => Page::Main,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Page;

    /// The table is the whole key map: a key that named two pages would silently pick one of
    /// them (and the help would print both), and a page's own key has to come back from it.
    #[test]
    fn every_page_key_is_bound_once() {
        let keys: Vec<char> = Page::ALL
            .iter()
            .filter_map(|page| page.spec().key)
            .collect();
        for (i, key) in keys.iter().enumerate() {
            let later = &keys[i + 1..];
            assert!(
                later.iter().all(|other| other != key),
                "{key} opens more than one page"
            );
            let page = Page::opened_by(*key).expect("the key names a page");
            assert_eq!(Page::Main.on_key(*key), Some(page));
            assert_eq!(page.on_key(*key), Some(Page::Main));
        }
    }
}
