use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};

use crate::{config::Theme, utils::gradient_color};

/// Parse `<tag>text</tag>` markup into styled `Vec<Span>`.
///
/// Supported tags:
/// - Theme colors: `<accent>`, `<text>`, `<muted>`, `<error>`, `<bg>`, `<surface>`
/// - Modifiers: `<b>` (bold), `<i>` (italic), `<dim>` (dimmed)
/// - Literal colors: `<#rrggbb>`, or any name accepted by `ratatui::style::Color::from_str`
/// - Gradient: `<gradient:preset>text</gradient>` or `<grad:preset>text</grad>` (per-char gradient coloring)
///   Presets: warm, cubehelix, rainbow, turbo, spectral, viridis
///
/// Text without tags is rendered as plain spans with no styling.
///
/// `base` is the starting style. Fields left unset by the markup (e.g. the
/// foreground color on tag-less text) inherit from `base`, while an explicit
/// tag always wins. Pass `Style::default()` to get the original behavior where
/// unstyled text is rendered with no color and falls back to the terminal
/// default foreground.
pub(super) fn parse_styled_with<'a>(text: &'a str, theme: &Theme, base: Style) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    let mut tag_stack: Vec<Style> = Vec::new();
    let mut current_style = base;
    let mut pos = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();

    while pos < len {
        if bytes[pos] == b'<' {
            let tag_start = pos + 1;
            let mut tag_end = tag_start;
            while tag_end < len && bytes[tag_end] != b'>' {
                tag_end += 1;
            }
            if tag_end >= len {
                spans.push(Span::styled(&text[pos..pos + 1], current_style));
                pos += 1;
                continue;
            }

            let tag_content = &text[tag_start..tag_end];
            pos = tag_end + 1;

            if tag_content.starts_with('/') {
                tag_stack.pop().inspect(|s| current_style = *s);
            } else if let Some(preset) = tag_content
                .strip_prefix("gradient:")
                .or_else(|| tag_content.strip_prefix("grad:"))
            {
                let close_tag = if tag_content.starts_with("gradient:") {
                    "</gradient>"
                } else {
                    "</grad>"
                };
                if let Some(rest) = text[pos..].find(close_tag) {
                    let inner = &text[pos..pos + rest];
                    pos = pos + rest + close_tag.len();

                    let char_count = inner.chars().count();
                    for (i, ch) in inner.chars().enumerate() {
                        let t = if char_count <= 1 {
                            0.0
                        } else {
                            i as f32 / (char_count - 1) as f32
                        };
                        let [r, g, b] = gradient_color(preset, t);
                        let style = current_style.fg(Color::Rgb(r, g, b));
                        let byte_start =
                            inner.char_indices().nth(i).map(|(idx, _)| idx).unwrap_or(0);
                        let char_len = ch.len_utf8();
                        spans.push(Span::styled(
                            &inner[byte_start..byte_start + char_len],
                            style,
                        ));
                    }
                } else {
                    // no closing tag found: skip the entire unclosed gradient
                    pos = len;
                }
            } else {
                tag_stack.push(current_style);
                current_style = apply_tag(tag_content, current_style, theme);
            }
        } else {
            let start = pos;
            while pos < len && bytes[pos] != b'<' {
                pos += 1;
            }
            let slice = &text[start..pos];
            if !slice.is_empty() {
                spans.push(Span::styled(slice, current_style));
            }
        }
    }

    spans
}

/// Parse markup into styled spans, leaving unstyled text with no color (the
/// terminal default foreground). See [`parse_styled_with`] for a variant that
/// seeds a base style so tag-less text gets a default color.
pub(super) fn parse_styled<'a>(text: &'a str, theme: &Theme) -> Vec<Span<'a>> {
    parse_styled_with(text, theme, Style::default())
}

fn apply_tag(tag: &str, current: Style, theme: &Theme) -> Style {
    match tag {
        "b" => current.add_modifier(Modifier::BOLD),
        "i" => current.add_modifier(Modifier::ITALIC),
        "dim" => current.add_modifier(Modifier::DIM),
        _ => {
            let is_theme_color = matches!(
                tag,
                "bg" | "surface" | "text" | "accent" | "muted" | "error" | "border"
            );
            if is_theme_color {
                current.fg(theme.field_color(tag))
            } else if let Ok(c) = tag.parse::<Color>() {
                current.fg(c)
            } else {
                current
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_tag_generates_per_char_spans() {
        let theme = Theme::default();
        // turbo: t=0 is dark, t=0.5 is greenish, t=1 is red — all different
        let spans = parse_styled("<gradient:turbo>ABC</gradient>", &theme);
        assert_eq!(spans.len(), 3);
        let c0 = spans[0].style.fg.unwrap();
        let c1 = spans[1].style.fg.unwrap();
        let c2 = spans[2].style.fg.unwrap();
        assert_ne!(c0, c1);
        assert_ne!(c1, c2);
    }

    #[test]
    fn gradient_tag_single_char() {
        let theme = Theme::default();
        let spans = parse_styled("<grad:warm>X</grad>", &theme);
        assert_eq!(spans.len(), 1);
        assert!(matches!(spans[0].style.fg, Some(Color::Rgb(_, _, _))));
    }

    #[test]
    fn mixed_tags() {
        let theme = Theme::default();
        let spans = parse_styled("hello <gradient:turbo>world</gradient>!", &theme);
        // "hello " = 1 span, "world" = 5 per-char gradient spans, "!" = 1 span
        assert_eq!(spans.len(), 7);
        assert_eq!(spans[0].content, "hello ");
        assert_eq!(spans[6].content, "!");
    }

    #[test]
    fn no_closing_tag_renders_nothing() {
        let theme = Theme::default();
        let spans = parse_styled("prefix <gradient:turbo>unclosed", &theme);
        // only "prefix " is rendered, unclosed gradient is skipped
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "prefix ");
    }

    #[test]
    fn nested_with_gradient() {
        let theme = Theme::default();
        let spans = parse_styled("<b><gradient:warm>hi</gradient></b>", &theme);
        assert_eq!(spans.len(), 2);
        // gradient chars should inherit bold from parent tag
        for s in &spans {
            assert!(s.style.add_modifier.contains(Modifier::BOLD));
        }
    }

    #[test]
    fn base_color_applies_to_tagless_text() {
        let theme = Theme::default();
        let spans = parse_styled_with("hi", &theme, Style::default().fg(Color::Yellow));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style.fg, Some(Color::Yellow));
    }

    #[test]
    fn explicit_tag_wins_over_base_color() {
        let theme = Theme::default();
        let spans = parse_styled_with(
            "<accent>x</accent>",
            &theme,
            Style::default().fg(Color::Yellow),
        );
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style.fg, Some(theme.accent));
    }
}
/// `cargo test --release --lib -- --ignored --nocapture styled_text`
///
/// Markup is parsed **on every draw** at ~10 call sites (block titles, breadcrumbs, table
/// headers, and once per navigation item), and the strings involved are almost all literals.
/// This measures both what that costs today and what reusing a pre-parsed result would cost.
#[cfg(test)]
mod bench {
    use super::*;
    use crate::config::ThemeRegistry;

    /// The shapes the render path actually hands in, taken from the real call sites.
    const TITLES: [&str; 6] = [
        " <accent> ► <b>AUTHENTICATION REQUIRED</b></accent>",
        "<accent>热门歌手</accent>",
        "<muted>歌手详情</muted>",
        "<text>我喜欢的音乐</text>",
        "本地音乐",
        "<accent>歌单</accent>",
    ];

    fn theme() -> Theme {
        ThemeRegistry::new(Default::default())
            .get("catppuccin-mocha")
            .cloned()
            .unwrap_or_default()
    }

    #[test]
    #[ignore]
    fn parsing_the_titles_on_each_draw_costs() {
        let theme = theme();

        let per_frame = crate::bench_util::time("解析 6 条标题标记（现状）", 20000, || {
            for title in TITLES {
                std::hint::black_box(parse_styled(title, &theme));
            }
        });
        println!(
            "  → 相当于每帧几十次调用时，约 {:.2}% 单核 @31 Hz",
            crate::bench_util::core_share(per_frame, 31.0)
        );

        // The ceiling for any caching scheme: hand out an already-parsed copy instead of
        // parsing again. (A `static` cache cannot hold this: `LazyLock::new` needs a const
        // closure, and `parse_styled` is a runtime call — a real cache would need `OnceLock`
        // keyed by the theme generation.)
        let parsed: Vec<Vec<Span>> = TITLES.iter().map(|t| parse_styled(t, &theme)).collect();
        let per_frame_reused = crate::bench_util::time("复用已解析结果（缓存上限）", 20000, || {
            for spans in &parsed {
                std::hint::black_box(spans.clone());
            }
        });
        println!(
            "  → 同样调用频率下约 {:.2}% 单核 @31 Hz（比值 {:.1}×）",
            crate::bench_util::core_share(per_frame_reused, 31.0),
            per_frame / per_frame_reused
        );
    }
}
