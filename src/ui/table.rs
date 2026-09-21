use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Cell, Row, Table, TableState},
};

use super::styled_text;
use crate::{
    config::{ColumnDef, Theme},
    state::TableMode,
    ui::scrollbar::render_scrollbar,
};

#[allow(clippy::too_many_arguments)]
pub(super) fn render_table(
    f: &mut Frame,
    headers: &[ColumnDef],
    rows: Vec<Row<'_>>,
    table_state: &mut TableState,
    table_mode: TableMode,
    colors: &Theme,
    area: Rect,
    row_count: usize,
    sel: usize,
) {
    if row_count == 0 || headers.is_empty() {
        return;
    }

    let [table_area, scrollbar_area] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]).areas(area);

    let header_cells: Vec<Cell> = headers
        .iter()
        .map(|h| {
            let spans = styled_text::parse_styled(&h.header, colors);
            Cell::from(Line::from(spans)).style(colors.looks().table_header.style())
        })
        .collect();
    let header = Row::new(header_cells)
        .style(Style::default().add_modifier(Modifier::BOLD))
        .height(1);

    let widths: Vec<Constraint> = headers.iter().map(|h| h.to_constraint()).collect();

    let table = Table::new(rows, widths).header(header).column_spacing(2);

    match table_mode {
        TableMode::Row => {
            // The highlight wins over the cells' own styles, so the readable colour has to
            // be named here: `bg` on `accent` is nearly invisible in the light palettes.
            let row_style = Style::default()
                .patch(colors.looks().table_selected.style())
                .add_modifier(Modifier::BOLD);

            let table = table.row_highlight_style(row_style).highlight_symbol("");

            f.render_stateful_widget(table, table_area, table_state);
        }
        TableMode::Cell => {
            let cell_highlight = Style::default()
                .fg(colors.on_accent())
                .bg(colors.accent)
                .add_modifier(Modifier::BOLD);

            let table = table.cell_highlight_style(cell_highlight);

            f.render_stateful_widget(table, table_area, table_state);
        }
    }

    render_scrollbar(f, row_count, sel, scrollbar_area, colors.muted);
}

#[cfg(test)]
mod selection_readability {
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, widgets::TableState};

    use super::*;

    fn columns() -> Vec<ColumnDef> {
        vec![ColumnDef {
            header: "TITLE".into(),
            field: "title".into(),
            width: None,
            min_width: None,
            ratio: None,
        }]
    }

    fn render_selected(name: &str) -> (Buffer, Theme) {
        let colors = crate::config::ThemeRegistry::new(Default::default())
            .get(name)
            .cloned()
            .unwrap_or_default();
        // The rows are dimmed with `muted` exactly like the real builders do.
        let rows: Vec<Row> = (0..3)
            .map(|i| {
                Row::new(vec![
                    Cell::from(format!("row{i}")).style(Style::default().fg(colors.muted)),
                ])
            })
            .collect();

        let mut state = TableState::default();
        state.select(Some(1));

        let mut terminal = Terminal::new(TestBackend::new(24, 5)).expect("backend");
        terminal
            .draw(|f| {
                render_table(
                    f,
                    &columns(),
                    rows,
                    &mut state,
                    TableMode::Row,
                    &colors,
                    f.area(),
                    3,
                    1,
                );
            })
            .expect("draw");
        (terminal.backend().buffer().clone(), colors)
    }

    /// The highlight style is applied over the cells, so its colour is the one a reader
    /// sees — `bg` on `accent` measured 1.06-1.57:1 in the light palettes, which is how the
    /// selected row became invisible there while looking fine in the dark ones.
    #[test]
    fn the_selected_row_is_readable_on_its_highlight() {
        for name in [
            "github-light",
            "gruvbox-light",
            "one-light",
            "solarized-light",
            "catppuccin-latte",
            "default",
            "dracula",
        ] {
            let (buffer, colors) = render_selected(name);
            let y = 2; // header row, then the selected one
            let mut cells = 0;
            for x in 0..buffer.area.width.saturating_sub(1) {
                let cell = &buffer[(x, y)];
                if cell.symbol().trim().is_empty() {
                    continue;
                }
                assert_eq!(
                    cell.fg,
                    colors.on_accent(),
                    "{name}: ({x},{y}) draws {:?} on {:?}",
                    cell.fg,
                    cell.bg
                );
                assert_eq!(
                    cell.bg, colors.accent,
                    "{name}: ({x},{y}) lost the highlight"
                );
                cells += 1;
            }
            assert!(cells > 0, "{name}: the selected row rendered nothing");
        }
    }

    /// Rows that are not selected keep the dim look they always had.
    #[test]
    fn unselected_rows_stay_dimmed() {
        let (buffer, colors) = render_selected("github-light");
        assert_eq!(buffer[(0, 1)].fg, colors.muted);
        assert_eq!(buffer[(0, 3)].fg, colors.muted);
    }
}
