use qrcode::{QrCode, render::unicode::Dense1x2};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{BlockStyle, block::CornerBlock, splash::LOGO};
use crate::{
    config::Theme,
    layout::LoginLayout,
    state::{LoginField, LoginMethod, LoginState},
    text_input::TextInput,
};

pub(super) fn draw(
    f: &mut Frame,
    login: &mut LoginState,
    bs: &BlockStyle<'_>,
    layout: &LoginLayout,
) {
    let colors = bs.colors;
    render_status(f, colors, layout.status);
    render_logo(f, colors, layout.logo);
    render_box(f, login, bs, layout.login_box);
}

fn render_logo(f: &mut Frame, colors: &Theme, area: Rect) {
    if area.width < 20 {
        return;
    }
    let rows = LOGO.len() as u16;
    let top = if area.height > rows {
        area.y + (area.height - rows) / 2
    } else {
        area.y
    };
    for (i, line) in LOGO.iter().enumerate() {
        let span = Span::styled(
            line.to_string(),
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        );
        f.render_widget(
            Paragraph::new(Line::from(span)).alignment(Alignment::Center),
            Rect {
                x: area.x,
                y: top + i as u16,
                width: area.width,
                height: 1,
            },
        );
    }
}

fn render_status(f: &mut Frame, colors: &Theme, area: Rect) {
    let line = Line::from(vec![
        Span::styled(
            "● ",
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
        Span::styled("ONLINE // RTT 36ms", Style::default().fg(colors.muted)),
    ]);
    f.render_widget(Paragraph::new(line).alignment(Alignment::Right), area);
}

fn render_box(f: &mut Frame, login: &mut LoginState, bs: &BlockStyle<'_>, area: Rect) {
    let colors = bs.colors;
    let box_width = area.width.saturating_sub(10).min(64);
    let box_x = area.x + (area.width.saturating_sub(box_width)) / 2;

    let content_rows: u16 = 30;
    let box_height = (8 + content_rows).min(area.height);
    let box_y = area.y + (area.height.saturating_sub(box_height)) / 2;

    let block = CornerBlock::from_color(bs, colors.bg).title(
        " <accent> ► <b>AUTHENTICATION REQUIRED</b></accent>",
        colors,
    );

    let box_area = Rect {
        x: box_x,
        y: box_y,
        width: box_width,
        height: box_height,
    };
    let inner = block.inner(box_area);
    f.render_widget(block, box_area);

    render_inner(f, login, colors, inner);
}

fn render_inner(f: &mut Frame, login: &mut LoginState, colors: &Theme, area: Rect) {
    let [
        tabs_area,
        _,
        content_area,
        err_area,
        notice_area,
        btn_area,
        footer_area,
    ] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(10),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    render_methods(f, login.method, colors, tabs_area);

    match login.method {
        LoginMethod::Qr => render_qr_content(f, login, colors, content_area),
        LoginMethod::Password | LoginMethod::Sms => render_form(f, login, colors, content_area),
        LoginMethod::Sign => render_sign_content(f, colors, content_area),
    }

    if let Some(err) = &login.error {
        let err_line = Line::from(Span::styled(
            format!(" ✗ {}", err),
            Style::default().fg(colors.error),
        ));
        f.render_widget(
            Paragraph::new(err_line).alignment(Alignment::Center),
            err_area,
        );
    }

    if let Some(notice) = &login.notice {
        let notice_line = Line::from(Span::styled(
            format!(" ● {}", notice),
            Style::default().fg(colors.accent),
        ));
        f.render_widget(
            Paragraph::new(notice_line).alignment(Alignment::Center),
            notice_area,
        );
    }

    if login.loading {
        let loading_line = Line::from(Span::styled(
            loading_label(login.method, login.focus),
            Style::default().fg(colors.muted),
        ));
        f.render_widget(
            Paragraph::new(loading_line).alignment(Alignment::Center),
            btn_area,
        );
    } else {
        render_button(f, colors, btn_area, action_label(login.method, login.focus));
    }
    render_footer(f, colors, footer_area, login.method, login.focus);
}

/// The tab strip above the form: `▶` and accent mark the method on screen, the others sit muted
/// behind a `·`. The label is a short one because the box is half the terminal wide.
fn render_methods(f: &mut Frame, current: LoginMethod, colors: &Theme, area: Rect) {
    let selected = Style::default()
        .fg(colors.accent)
        .add_modifier(Modifier::BOLD);
    let other = Style::default().fg(colors.muted);

    let mut spans = Vec::new();
    for method in LoginMethod::ALL {
        if !spans.is_empty() {
            spans.push(Span::styled(" · ", other));
        }
        if method == current {
            spans.push(Span::styled("▶ ", selected));
            spans.push(Span::styled(method.label(), selected));
        } else {
            spans.push(Span::styled(method.label(), other));
        }
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
        area,
    );
}

/// The action row: what `Enter` does to the method on screen. The SMS tab has two steps, so its
/// label follows the caret rather than sitting on one of them.
fn action_label(method: LoginMethod, focus: LoginField) -> &'static str {
    match (method, focus) {
        (LoginMethod::Qr, _) => "► GENERATE QR CODE",
        (LoginMethod::Password, _) => "► LOGIN",
        (LoginMethod::Sms, LoginField::Phone) => "► SEND CODE",
        (LoginMethod::Sms, _) => "► LOGIN",
        (LoginMethod::Sign, _) => "► DAILY SIGN-IN",
    }
}

fn loading_label(method: LoginMethod, focus: LoginField) -> &'static str {
    match (method, focus) {
        (LoginMethod::Qr, _) => " ◌ CREATING QR CODE ...",
        (LoginMethod::Sms, LoginField::Phone) => " ◌ SENDING CODE ...",
        (LoginMethod::Sms, _) => " ◌ SIGNING IN ...",
        (LoginMethod::Password, _) => " ◌ SIGNING IN ...",
        (LoginMethod::Sign, _) => " ◌ CHECKING IN ...",
    }
}

/// What the page's keys do, per method. The box is half the terminal wide, so the hints have to
/// fit in about 38 columns: they name the keys of the method on screen and nothing else, and the
/// method switching is spelled out in the help popup.
fn render_footer(
    f: &mut Frame,
    colors: &Theme,
    area: Rect,
    method: LoginMethod,
    focus: LoginField,
) {
    let hint = match (method, focus) {
        (LoginMethod::Qr, _) => "ENTER 生成二维码 · ESC 返回",
        (LoginMethod::Password, _) => "TAB 切框 · ←→ 切方式 · ENTER 登录",
        (LoginMethod::Sms, LoginField::Phone) => "TAB 切框 · ←→ 切方式 · ENTER 发送",
        (LoginMethod::Sms, _) => "TAB 切框 · ←→ 切方式 · ENTER 登录",
        (LoginMethod::Sign, _) => "←→ 切方式 · ENTER 签到 · ESC 返回",
    };
    let line = Line::from(Span::styled(hint, Style::default().fg(colors.muted)));
    f.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

/// The rows of a method's form: label, the input it edits, and whether the value is masked.
fn form_rows(method: LoginMethod) -> &'static [(&'static str, LoginField)] {
    match method {
        LoginMethod::Password => &[
            ("账号", LoginField::Account),
            ("密码", LoginField::Password),
        ],
        LoginMethod::Sms => &[("手机号", LoginField::Phone), ("验证码", LoginField::Code)],
        LoginMethod::Qr | LoginMethod::Sign => &[],
    }
}

/// The password and SMS forms. Both boxes are drawn the same way; the one the caret is in says
/// so with a marker, an accent label and accent brackets, and gets the terminal's own cursor if
/// the user types. The value is clipped to the box, and the password is drawn as bullets: the
/// characters never reach the screen.
fn render_form(f: &mut Frame, login: &mut LoginState, colors: &Theme, area: Rect) {
    /// Display columns for the label column ("手机号" plus its gap).
    const LABEL: u16 = 7;
    /// Display columns for the caret marker, on every row whether or not it carries one.
    const MARKER: u16 = 2;

    let rows = form_rows(login.method);
    let form_width = area.width.saturating_sub(4).min(44);
    let x = area.x + area.width.saturating_sub(form_width) / 2;
    let value_width = form_width.saturating_sub(MARKER + LABEL + 4) as usize;
    let height = 2 + rows.len() as u16 * 2;
    let top = area.y + area.height.saturating_sub(height) / 2;

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            login.method.heading(),
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center),
        Rect {
            x,
            y: top,
            width: form_width,
            height: 1,
        },
    );

    for (i, (label, field)) in rows.iter().enumerate() {
        let field = *field;
        let focused = field == login.focus;
        let y = top + 2 + i as u16 * 2;
        let (marker, label_style, bracket_style, value_style) = if focused {
            (
                "▶ ",
                Style::default().fg(colors.accent),
                Style::default().fg(colors.accent),
                Style::default().fg(colors.accent),
            )
        } else {
            (
                "  ",
                Style::default().fg(colors.muted),
                Style::default().fg(colors.muted),
                Style::default().fg(colors.text),
            )
        };

        let input = login.input(field);
        let shown = masked_text(input, field.masked(), value_width);
        let pad = value_width.saturating_sub(UnicodeWidthStr::width(shown.as_str()));
        let label_pad = LABEL.saturating_sub(UnicodeWidthStr::width(*label) as u16);

        let line = Line::from(vec![
            Span::styled(marker, label_style),
            Span::styled(
                format!("{label}{}", " ".repeat(label_pad as usize)),
                label_style,
            ),
            Span::styled("[ ", bracket_style),
            Span::styled(shown, value_style),
            Span::styled(" ".repeat(pad), value_style),
            Span::styled(" ]", bracket_style),
        ]);
        f.render_widget(
            Paragraph::new(line),
            Rect {
                x,
                y,
                width: form_width,
                height: 1,
            },
        );

        // The caret goes inside the brackets, and only while the box is the one being edited:
        // during a request the form is the server's, not the user's.
        input.show_cursor_at(
            f,
            x + MARKER + LABEL + 2,
            y,
            focused && !login.loading,
            field.masked(),
        );
    }
}

/// The check-in tab: no form, one key.
fn render_sign_content(f: &mut Frame, colors: &Theme, area: Rect) {
    let top = area.y + area.height.saturating_sub(4) / 2;
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            LoginMethod::Sign.heading(),
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center),
        Rect {
            y: top,
            height: 1,
            ..area
        },
    );

    let msg = Line::from(Span::styled(
        "  Press ENTER to check in for today's 云贝  ",
        Style::default()
            .fg(colors.muted)
            .add_modifier(Modifier::SLOW_BLINK),
    ));
    f.render_widget(
        Paragraph::new(msg).alignment(Alignment::Center),
        Rect {
            y: top + 2,
            height: 1,
            ..area
        },
    );
}

/// A field's value as it may be drawn: bullets for the password, and clipped to the box so a
/// long account cannot push the closing bracket off the line.
fn masked_text(input: &TextInput, masked: bool, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for ch in input.value.chars() {
        let ch = if masked { '•' } else { ch };
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(1);
        if used + char_width > width {
            break;
        }
        out.push(ch);
        used += char_width;
    }
    out
}

fn render_qr_content(f: &mut Frame, login: &mut LoginState, colors: &Theme, area: Rect) {
    if login.qr_url.is_empty() {
        let msg = Line::from(Span::styled(
            "  Press ENTER to generate QR code  ",
            Style::default()
                .fg(colors.muted)
                .add_modifier(Modifier::SLOW_BLINK),
        ));
        let centered_row = Rect {
            x: area.x,
            y: area.y + area.height / 2,
            width: area.width,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(msg).alignment(Alignment::Center),
            centered_row,
        );
        return;
    }

    // Encode only once per url; QR (Reed-Solomon) generation is CPU-heavy and
    // the url only changes on login events.
    if login
        .qr_cache
        .as_ref()
        .is_none_or(|(url, _)| url != &login.qr_url)
    {
        match QrCode::new(login.qr_url.as_bytes()) {
            Ok(code) => {
                let qr_str = code.render::<Dense1x2>().quiet_zone(false).build();
                login.qr_cache = Some((
                    login.qr_url.clone(),
                    qr_str.lines().map(|l| l.to_string()).collect(),
                ));
            }
            Err(_) => {
                let msg = Line::from(Span::styled(
                    "  Failed to generate QR code  ",
                    Style::default().fg(colors.error),
                ));
                f.render_widget(Paragraph::new(msg).alignment(Alignment::Center), area);
                return;
            }
        }
    }

    let mut lines: Vec<Line> = login
        .qr_cache
        .as_ref()
        .map(|(_, rendered)| rendered)
        .into_iter()
        .flatten()
        .map(|l| Line::from(Span::styled(l.clone(), Style::default().fg(colors.accent))))
        .collect();

    let hint = if login.qr_status_text.is_empty() {
        "Scan with Netease Cloud Music App"
    } else {
        &login.qr_status_text
    };
    lines.push(Line::from(Span::styled(
        hint,
        Style::default().fg(colors.muted),
    )));

    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

fn render_button(f: &mut Frame, colors: &Theme, area: Rect, text: &str) {
    let inner = area.width as usize;
    let width = UnicodeWidthStr::width(text);
    let pad_left = inner.saturating_sub(width) / 2;
    let pad_right = inner.saturating_sub(width).saturating_sub(pad_left);

    let line = Line::from(vec![Span::styled(
        format!(
            "{:pad_left$}{}{:pad_right$}",
            "",
            text,
            "",
            pad_left = pad_left,
            pad_right = pad_right
        ),
        Style::default()
            .fg(colors.bg)
            .bg(colors.accent)
            .add_modifier(Modifier::BOLD),
    )]);
    f.render_widget(Paragraph::new(line), area);
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{
        config::{BorderConfig, ThemeRegistry},
        layout,
        state::LoginMethod,
    };

    /// Render one login state and flatten it the way a reader sees it.
    fn screen(state: &mut LoginState) -> String {
        let colors = ThemeRegistry::new(Default::default())
            .get("catppuccin-mocha")
            .cloned()
            .unwrap_or_default();
        let border = BorderConfig::default();
        let bs = BlockStyle {
            colors: &colors,
            border: &border,
            tick: 0,
        };
        let mut terminal = Terminal::new(TestBackend::new(100, 40)).expect("backend");
        terminal
            .draw(|f| {
                let lay = layout::login(f.area());
                draw(f, state, &bs, &lay);
            })
            .expect("draw");
        let buf = terminal.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                // A wide glyph (CJK) covers two cells and leaves the second one empty; empty
                // cells are skipped so a word is not split apart in the flattened text.
                (0..buf.area.width)
                    .filter(|x| !buf[(*x, y)].symbol().is_empty())
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join(
                "
",
            )
    }

    #[test]
    fn the_password_is_never_drawn_in_the_clear() {
        let mut state = LoginState::default();
        state.open_password("someone@example.com", "hunter2");
        let drawn = screen(&mut state);
        assert!(!drawn.contains("hunter2"), "密码不能明文上屏：\n{drawn}");
        assert!(drawn.contains('•'), "密码应当画成掩码：\n{drawn}");
        assert!(
            drawn.contains("someone@example.com"),
            "账号应当照常显示：\n{drawn}"
        );
    }

    #[test]
    fn the_qr_tab_keeps_its_own_content() {
        // The QR flow is the one method that must not regress; its tab still asks for ENTER.
        let mut state = LoginState::default();
        state.open(LoginMethod::Qr);
        let drawn = screen(&mut state);
        assert!(
            drawn.contains("AUTHENTICATION REQUIRED"),
            "登录页外壳应仍在：\n{drawn}"
        );
        assert!(
            drawn.contains("Press ENTER to generate QR code"),
            "二维码标签内容应仍在：\n{drawn}"
        );
    }
}
