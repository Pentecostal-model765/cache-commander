use crate::tree::node::TreeNode;
use crate::ui::theme;
use humansize::{BINARY, format_size};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub fn render_delete_confirm(f: &mut Frame, items: &[&TreeNode]) {
    let area = centered_rect(50, 40, f.area());

    f.render_widget(Clear, area);

    let total_size: u64 = items.iter().map(|n| n.size).sum();
    let count = items.len();

    let block = Block::default()
        .title(format!(
            " Delete {count} item{}? ",
            if count == 1 { "" } else { "s" }
        ))
        .title_style(theme::DANGER)
        .borders(Borders::ALL)
        .border_style(theme::DIALOG_BORDER);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for item in items.iter().take(10) {
        lines.push(Line::from(vec![
            Span::styled("  ", theme::NORMAL),
            Span::styled(&item.name, theme::NORMAL),
            Span::styled(format!(" ({})", format_size(item.size, BINARY)), theme::DIM),
        ]));
    }

    if count > 10 {
        lines.push(Line::from(Span::styled(
            format!("  ...and {} more", count - 10),
            theme::DIM,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("  Total: {} will be freed", format_size(total_size, BINARY)),
        theme::NORMAL,
    )));
    lines.push(Line::from(""));

    // Safety summary
    let all_safe = items
        .iter()
        .all(|n| n.kind != crate::tree::node::CacheKind::Unknown);
    if all_safe {
        lines.push(Line::from(Span::styled(
            "  ● All items are safe to delete (re-downloadable)",
            theme::SAFE,
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "  ◐ Some items have unknown safety — inspect before deleting",
            theme::CAUTION,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  [y]", theme::KEY),
        Span::styled(" confirm   ", theme::NORMAL),
        Span::styled("[n]", theme::DIM),
        Span::styled(" cancel", theme::NORMAL),
    ]));

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}

pub fn render_help(f: &mut Frame) {
    let area = centered_rect(60, 70, f.area());

    f.render_widget(Clear, area);

    let block = Block::default()
        .title(" Help ")
        .title_style(theme::TITLE)
        .borders(Borders::ALL)
        .border_style(theme::HELP_BORDER);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let keys = vec![
        ("↑/k", "Move up"),
        ("↓/j", "Move down"),
        ("→/l", "Expand"),
        ("←/h", "Collapse / go to parent"),
        ("Enter", "Toggle expand"),
        ("g", "Jump to top"),
        ("G", "Jump to bottom"),
        ("", ""),
        ("Space", "Mark / unmark item"),
        ("u", "Unmark all"),
        ("d/D", "Delete marked items"),
        ("s", "Cycle sort (size/name/modified)"),
        ("r", "Refresh selected"),
        ("R", "Refresh all"),
        ("/", "Search / filter"),
        ("c", "Copy upgrade command to clipboard"),
        ("f", "Cycle status filter (vuln/outdated)"),
        ("m", "Mark all visible items"),
        ("Esc", "Clear filter / cancel"),
        ("", ""),
        ("v", "Scan selected for CVEs"),
        ("V", "Scan all for CVEs"),
        ("o", "Check selected for updates"),
        ("O", "Check all for updates"),
        ("?", "Toggle help"),
        ("q", "Quit"),
    ];

    let mut lines: Vec<Line> = vec![Line::from("")];
    for (key, desc) in keys {
        if key.is_empty() {
            lines.push(Line::from(""));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<10}", key), theme::KEY),
                Span::styled(desc, theme::NORMAL),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  By Julien Simon <julien@julien.org>",
        theme::DIM,
    )));
    lines.push(Line::from(Span::styled(
        "  Docs & code: github.com/juliensimon/cache-commander",
        theme::DIM,
    )));

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)])
        .flex(Flex::Center)
        .split(area);
    Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::node::{CacheKind, TreeNode};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;

    fn make_node(name: &str, kind: CacheKind, size: u64) -> TreeNode {
        let mut n = TreeNode::new(PathBuf::from(format!("/tmp/{name}")), 0, None);
        n.name = name.into();
        n.kind = kind;
        n.size = size;
        n
    }

    fn render_dialog<F>(draw: F) -> String
    where
        F: FnOnce(&mut Frame),
    {
        let backend = TestBackend::new(100, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn delete_confirm_single_item_uses_singular_title_and_all_safe() {
        let node = make_node("serde 1.0.200", CacheKind::Cargo, 1024 * 1024);
        let out = render_dialog(|f| render_delete_confirm(f, &[&node]));
        assert!(out.contains("Delete 1 item?"), "singular title:\n{out}");
        assert!(out.contains("serde 1.0.200"), "item name:\n{out}");
        assert!(out.contains("1 MiB"), "item size:\n{out}");
        assert!(out.contains("Total:"), "total line:\n{out}");
        assert!(out.contains("safe to delete"), "safe badge:\n{out}");
        assert!(out.contains("[y]"), "y key:\n{out}");
        assert!(out.contains("[n]"), "n key:\n{out}");
        assert!(
            !out.contains("and "),
            "single item should not show 'and N more':\n{out}"
        );
    }

    #[test]
    fn delete_confirm_plural_title_and_total_freed() {
        let a = make_node("a", CacheKind::Cargo, 1024 * 1024);
        let b = make_node("b", CacheKind::Npm, 2 * 1024 * 1024);
        let out = render_dialog(|f| render_delete_confirm(f, &[&a, &b]));
        assert!(out.contains("Delete 2 items?"), "plural title:\n{out}");
        assert!(out.contains("3 MiB"), "summed total:\n{out}");
    }

    #[test]
    fn delete_confirm_truncates_to_ten_and_shows_more() {
        let nodes: Vec<TreeNode> = (0..15)
            .map(|i| make_node(&format!("pkg-{i}"), CacheKind::Cargo, 1024))
            .collect();
        let refs: Vec<&TreeNode> = nodes.iter().collect();
        let out = render_dialog(|f| render_delete_confirm(f, &refs));
        assert!(out.contains("Delete 15 items?"));
        assert!(out.contains("pkg-0"), "first shown:\n{out}");
        assert!(out.contains("pkg-9"), "tenth shown:\n{out}");
        assert!(
            !out.contains("pkg-10"),
            "eleventh must not appear in first-10 list:\n{out}"
        );
        assert!(out.contains("and 5 more"), "overflow hint:\n{out}");
    }

    #[test]
    fn delete_confirm_unknown_kind_shows_caution_summary() {
        let node = make_node("mystery", CacheKind::Unknown, 1024);
        let out = render_dialog(|f| render_delete_confirm(f, &[&node]));
        assert!(
            out.contains("unknown safety"),
            "expected caution banner:\n{out}"
        );
        assert!(
            !out.contains("All items are safe"),
            "should not claim safety:\n{out}"
        );
    }

    #[test]
    fn help_dialog_lists_all_keybindings_and_author() {
        // Use a tall terminal so the 70%-height centered dialog fits the full
        // keybinding list + author credit without ratatui clipping the bottom.
        let backend = TestBackend::new(120, 60);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(render_help).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        assert!(out.contains("Help"), "title:\n{out}");
        for k in &[
            "↑/k", "↓/j", "g", "G", "Space", "d/D", "/", "v", "V", "o", "O", "?", "q",
        ] {
            assert!(out.contains(k), "missing key {k}:\n{out}");
        }
        for d in &["Move up", "Move down", "Jump to top", "Quit"] {
            assert!(out.contains(d), "missing desc {d}:\n{out}");
        }
        assert!(out.contains("Julien Simon"), "author credit:\n{out}");
        assert!(
            out.contains("github.com/juliensimon/cache-commander"),
            "repo link:\n{out}"
        );
    }

    #[test]
    fn centered_rect_is_centered_and_smaller() {
        let area = Rect::new(0, 0, 100, 40);
        let r = centered_rect(50, 40, area);
        assert_eq!(r.width, 50);
        assert_eq!(r.height, 16);
        assert_eq!(r.x, 25); // (100-50)/2
        assert_eq!(r.y, 12); // (40-16)/2
    }
}
