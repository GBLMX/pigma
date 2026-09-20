//! Main screen area layout: splits the frame into topbar, navigation, content and
//! player-bar regions (plus the splash layout).

use ratatui::layout::{Constraint, Flex, Layout, Rect};

use crate::config::NavPosition;

pub struct SplashLayout {
    pub logo: Rect,
    pub progress: Rect,
    pub logs: Rect,
    pub tag: Rect,
}

/// `logo_rows` is the height of the ASCII logo: the art is the only place its height is
/// written down, and the splash must not have a second opinion about it.
pub fn splash(area: Rect, logo_rows: u16) -> SplashLayout {
    let [logo_area, progress_area, logs_area, tag_area] = Layout::vertical([
        Constraint::Length(logo_rows),
        Constraint::Length(2),
        Constraint::Length(5),
        Constraint::Length(1),
    ])
    .flex(Flex::SpaceAround)
    .areas(area);

    SplashLayout {
        logo: logo_area,
        progress: progress_area,
        logs: logs_area,
        tag: tag_area,
    }
}

pub struct LoginLayout {
    pub status: Rect,
    pub logo: Rect,
    pub login_box: Rect,
}

pub fn login(area: Rect) -> LoginLayout {
    let [status_area, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(26)])
        .flex(Flex::Center)
        .spacing(1)
        .areas(area);

    let [logo_area, box_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(body);

    LoginLayout {
        status: status_area,
        logo: logo_area,
        login_box: box_area,
    }
}

pub struct LayoutAreas {
    pub topbar: Rect,
    pub sidebar: Rect,
    pub breadcrumb: Rect,
    pub nav: Rect,
    pub content: Rect,
    pub playerbar: Rect,
}

/// The main page: topbar, navigation, content and player bar.
pub fn main(area: Rect, nav_position: NavPosition) -> LayoutAreas {
    match nav_position {
        NavPosition::Top => {
            let [topbar, nav, middle, playerbar] = Layout::vertical([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(5),
            ])
            .areas(area);

            LayoutAreas {
                topbar,
                sidebar: Rect::default(),
                breadcrumb: Rect::default(),
                nav,
                content: middle,
                playerbar,
            }
        }
        NavPosition::Bottom => {
            let [topbar, middle, nav, playerbar] = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
                Constraint::Length(5),
            ])
            .areas(area);

            LayoutAreas {
                topbar,
                sidebar: Rect::default(),
                breadcrumb: Rect::default(),
                nav,
                content: middle,
                playerbar,
            }
        }
        NavPosition::Left | NavPosition::Right => {
            let [topbar, middle, playerbar] = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(5),
            ])
            .areas(area);

            // Hide the sidebar when the terminal is narrower than 60 columns; content fills the whole area
            let (sidebar, right) = if area.width < 60 {
                (Rect::default(), middle)
            } else {
                match nav_position {
                    NavPosition::Left => {
                        let [sidebar, right] =
                            Layout::horizontal([Constraint::Length(26), Constraint::Min(40)])
                                .areas(middle);
                        (sidebar, right)
                    }
                    NavPosition::Right => {
                        let [right, sidebar] =
                            Layout::horizontal([Constraint::Min(40), Constraint::Length(26)])
                                .areas(middle);
                        (sidebar, right)
                    }
                    _ => unreachable!(),
                }
            };

            let [breadcrumb, content] =
                Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(right);

            LayoutAreas {
                topbar,
                sidebar,
                breadcrumb,
                nav: Rect::default(),
                content,
                playerbar,
            }
        }
    }
}

/// A page that is one content area between the topbar and the player bar: the lyrics and the
/// queue. It has no navigation area, so the navigation position is taken and ignored — every
/// shell page's layout has the same shape for the table.
pub fn content(area: Rect, _nav_position: NavPosition) -> LayoutAreas {
    let [topbar, middle, playerbar] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(10),
        Constraint::Length(5),
    ])
    .areas(area);

    LayoutAreas {
        topbar,
        sidebar: Rect::default(),
        breadcrumb: Rect::default(),
        nav: Rect::default(),
        content: middle,
        playerbar,
    }
}

/// The artist page inside the shell's content area: the profile band on top, then the hot
/// songs and the albums.
pub struct ArtistLayout {
    pub profile: Rect,
    pub songs: Rect,
    pub albums: Rect,
}

/// Height of the profile band: a portrait, the name and sizes lines, and a few lines of
/// biography. It is fixed rather than proportional so the two lists below it stay where they
/// were when a biography is long — the band clips its text instead of growing.
const PROFILE_HEIGHT: u16 = 9;

/// Width from which the two lists share the area instead of stacking. Below it, four columns
/// for the songs and two for the albums is not enough to read either of them.
const SIDE_BY_SIDE_WIDTH: u16 = 100;

/// The artist page. The lists split the space under the band: side by side on a wide terminal
/// (songs first, since they are what a reader comes for) and stacked when there is no room for
/// two tables next to each other.
pub fn artist(area: Rect) -> ArtistLayout {
    let [profile, lists] =
        Layout::vertical([Constraint::Length(PROFILE_HEIGHT), Constraint::Min(2)]).areas(area);

    let (songs, albums) = if area.width >= SIDE_BY_SIDE_WIDTH {
        let [songs, albums] =
            Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
                .areas(lists);
        (songs, albums)
    } else {
        let [songs, albums] =
            Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(lists);
        (songs, albums)
    };

    ArtistLayout {
        profile,
        songs,
        albums,
    }
}
