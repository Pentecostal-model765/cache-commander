use crate::config::Config;
use crate::scanner::ScanResult;
use crate::tree::state::TreeState;
use crate::ui::{detail_panel, dialogs, tree_panel};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

#[derive(Debug, PartialEq)]
pub enum AppMode {
    Normal,
    Deleting,
    Help,
    Filtering,
    MarkingAll,
}

pub struct App {
    pub tree: TreeState,
    pub config: Config,
    pub mode: AppMode,
    pub filter_text: String,
    pub should_quit: bool,
    pub scan_rx: mpsc::Receiver<ScanResult>,
    pub scan_tx: mpsc::Sender<crate::scanner::ScanRequest>,
    pub status_msg: Option<String>,
    pub vuln_results: HashMap<PathBuf, crate::security::SecurityInfo>,
    pub version_results: HashMap<PathBuf, crate::security::VersionInfo>,
    pub node_status: HashMap<PathBuf, crate::security::NodeStatus>,
    delete_candidates: Vec<std::path::PathBuf>,
    pub mark_all_count: usize,
    auto_vulnscan_pending: bool,
    auto_versioncheck_pending: bool,
    vulnscan_in_progress: bool,
    versioncheck_in_progress: bool,
    pub brew_outdated_results: HashMap<String, crate::providers::homebrew::BrewOutdatedEntry>,
    brew_outdated_in_progress: bool,
    auto_brew_outdated_pending: bool,
}

impl App {
    pub fn new(
        config: Config,
        scan_rx: mpsc::Receiver<ScanResult>,
        scan_tx: mpsc::Sender<crate::scanner::ScanRequest>,
    ) -> Self {
        let tree = TreeState::new(config.sort_by, config.sort_desc);
        let auto_vuln = config.vulncheck.enabled;
        let auto_ver = config.versioncheck.enabled;
        Self {
            tree,
            config,
            mode: AppMode::Normal,
            filter_text: String::new(),
            should_quit: false,
            scan_rx,
            scan_tx,
            status_msg: None,
            vuln_results: HashMap::new(),
            version_results: HashMap::new(),
            node_status: HashMap::new(),
            delete_candidates: Vec::new(),
            mark_all_count: 0,
            auto_vulnscan_pending: auto_vuln,
            auto_versioncheck_pending: auto_ver,
            vulnscan_in_progress: false,
            versioncheck_in_progress: false,
            brew_outdated_results: HashMap::new(),
            brew_outdated_in_progress: false,
            auto_brew_outdated_pending: true,
        }
    }

    pub fn init(&self) {
        let roots = self.config.roots.clone();
        let _ = self
            .scan_tx
            .send(crate::scanner::ScanRequest::ScanRoots(roots));
    }

    pub fn tick(&mut self) {
        // Process scan results
        while let Ok(result) = self.scan_rx.try_recv() {
            match result {
                ScanResult::RootsScanned(nodes) => {
                    self.tree.set_roots(nodes);
                }
                ScanResult::ChildrenScanned(parent_path, children) => {
                    if let Some(parent_idx) =
                        self.tree.nodes.iter().position(|n| n.path == parent_path)
                    {
                        self.tree.insert_children(parent_idx, children);
                    }
                    if !self.brew_outdated_results.is_empty() {
                        self.recompute_node_status();
                        self.tree.recompute_dimmed(&self.node_status);
                    }
                }
                ScanResult::SizeUpdated(path, size) => {
                    if let Some(node) = self.tree.nodes.iter_mut().find(|n| n.path == path) {
                        node.size = size;
                    }
                }
                ScanResult::VulnsScanned(scanned, results) => {
                    self.vuln_results.extend(results);
                    self.vulnscan_in_progress = false;
                    self.recompute_node_status();
                    self.tree.recompute_dimmed(&self.node_status);
                    let vuln_count = self
                        .vuln_results
                        .values()
                        .map(|s| s.vulns.len())
                        .sum::<usize>();
                    self.status_msg = Some(if vuln_count > 0 {
                        format!(
                            "Scanned {} packages — {} vulnerabilit{} found",
                            scanned,
                            vuln_count,
                            if vuln_count == 1 { "y" } else { "ies" }
                        )
                    } else {
                        format!("Scanned {} packages — no vulnerabilities found", scanned)
                    });
                }
                ScanResult::VersionsChecked(checked, results) => {
                    self.version_results.extend(results);
                    self.versioncheck_in_progress = false;
                    self.recompute_node_status();
                    self.tree.recompute_dimmed(&self.node_status);
                    let outdated = self
                        .version_results
                        .values()
                        .filter(|v| v.is_outdated)
                        .count();
                    self.status_msg = Some(if outdated > 0 {
                        format!("Checked {} packages — {} outdated", checked, outdated)
                    } else {
                        format!("Checked {} packages — all up to date", checked)
                    });
                }
                ScanResult::BrewOutdatedCompleted(results) => {
                    let outdated_count = results.len();
                    self.brew_outdated_results = results;
                    self.brew_outdated_in_progress = false;
                    self.recompute_node_status();
                    self.tree.recompute_dimmed(&self.node_status);
                    if outdated_count > 0 {
                        self.status_msg = Some(format!(
                            "brew: {} outdated package{}",
                            outdated_count,
                            if outdated_count == 1 { "" } else { "s" }
                        ));
                    }
                }
            }
        }

        // Auto-scan on startup when CLI flags are set
        if (self.auto_vulnscan_pending || self.auto_versioncheck_pending)
            && !self.tree.nodes.is_empty()
        {
            let roots = self.config.roots.clone();
            if self.auto_vulnscan_pending {
                self.auto_vulnscan_pending = false;
                self.vulnscan_in_progress = true;
                let _ = self
                    .scan_tx
                    .send(crate::scanner::ScanRequest::ScanVulns(roots.clone()));
            }
            if self.auto_versioncheck_pending {
                self.auto_versioncheck_pending = false;
                self.versioncheck_in_progress = true;
                let _ = self
                    .scan_tx
                    .send(crate::scanner::ScanRequest::CheckVersions(roots));
            }
        }

        // Auto-trigger brew outdated when Homebrew caches are among configured roots
        if self.auto_brew_outdated_pending && !self.tree.nodes.is_empty() {
            self.auto_brew_outdated_pending = false;
            let has_homebrew = self
                .config
                .roots
                .iter()
                .any(|r| r.join("Homebrew").is_dir() || r.ends_with("Homebrew"));
            if has_homebrew {
                self.brew_outdated_in_progress = true;
                let _ = self.scan_tx.send(crate::scanner::ScanRequest::BrewOutdated);
            }
        }
    }

    pub fn handle_event(&mut self) -> bool {
        if event::poll(Duration::from_millis(60)).unwrap_or(false)
            && let Ok(Event::Key(key)) = event::read()
        {
            self.process_key(key);
        }
        self.tick();
        self.should_quit
    }

    pub fn process_key(&mut self, key: KeyEvent) {
        match self.mode {
            AppMode::Normal => self.handle_normal_key(key),
            AppMode::Deleting => self.handle_delete_key(key),
            AppMode::Help => self.handle_help_key(key),
            AppMode::Filtering => self.handle_filter_key(key),
            AppMode::MarkingAll => self.handle_mark_all_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true
            }
            KeyCode::Up | KeyCode::Char('k') => self.tree.move_up(),
            KeyCode::Down | KeyCode::Char('j') => self.tree.move_down(),
            KeyCode::Right | KeyCode::Char('l') => {
                if let Some(idx) = self.tree.expand() {
                    let path = self.tree.nodes[idx].path.clone();
                    let _ = self
                        .scan_tx
                        .send(crate::scanner::ScanRequest::ExpandNode(path));
                }
            }
            KeyCode::Left | KeyCode::Char('h') => self.tree.collapse(),
            KeyCode::Enter => {
                if let Some(idx) = self.tree.toggle_expand() {
                    let path = self.tree.nodes[idx].path.clone();
                    let _ = self
                        .scan_tx
                        .send(crate::scanner::ScanRequest::ExpandNode(path));
                }
            }
            KeyCode::Char('g') => self.tree.go_top(),
            KeyCode::Char('G') => self.tree.go_bottom(),
            KeyCode::Char(' ') => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.tree.marked.clear();
                } else {
                    self.tree.toggle_mark();
                }
            }
            KeyCode::Char('u') => self.tree.marked.clear(),
            KeyCode::Char('v') => {
                if let Some(idx) = self.tree.selected_node_index() {
                    self.vulnscan_in_progress = true;
                    let path = self.tree.nodes[idx].path.clone();
                    let _ = self
                        .scan_tx
                        .send(crate::scanner::ScanRequest::ScanVulns(vec![path]));
                }
            }
            KeyCode::Char('V') => {
                self.vulnscan_in_progress = true;
                let _ = self.scan_tx.send(crate::scanner::ScanRequest::ScanVulns(
                    self.config.roots.clone(),
                ));
            }
            KeyCode::Char('o') => {
                if let Some(idx) = self.tree.selected_node_index() {
                    self.versioncheck_in_progress = true;
                    let path = self.tree.nodes[idx].path.clone();
                    let _ = self
                        .scan_tx
                        .send(crate::scanner::ScanRequest::CheckVersions(vec![path]));
                }
            }
            KeyCode::Char('O') => {
                self.versioncheck_in_progress = true;
                let _ = self
                    .scan_tx
                    .send(crate::scanner::ScanRequest::CheckVersions(
                        self.config.roots.clone(),
                    ));
            }
            KeyCode::Char('d') | KeyCode::Char('D') if !self.tree.marked.is_empty() => {
                self.delete_candidates = self
                    .tree
                    .marked
                    .iter()
                    .filter_map(|&idx| self.tree.nodes.get(idx).map(|n| n.path.clone()))
                    .collect();
                if self.config.confirm_delete {
                    self.mode = AppMode::Deleting;
                } else {
                    self.perform_delete();
                }
            }
            KeyCode::Char('c') => {
                if let Some(cmd) = self.upgrade_command_for_selected() {
                    if copy_to_clipboard(&cmd) {
                        self.status_msg = Some(format!("Copied: {}", cmd));
                    } else {
                        self.status_msg = Some(format!("→ {}", cmd));
                    }
                } else {
                    self.status_msg = Some("No upgrade command for this item".into());
                }
            }
            KeyCode::Char('s') => self.tree.cycle_sort(),
            KeyCode::Char('f') => {
                if self.node_status.is_empty() {
                    self.status_msg =
                        Some("Run vuln scan (v/V) or version check (o/O) first".into());
                } else {
                    self.tree.filter_mode = self.tree.filter_mode.cycle();
                    self.tree.recompute_dimmed(&self.node_status);
                    self.tree.snap_selection_to_non_dimmed();
                    if self.tree.filter_mode != crate::tree::state::FilterMode::None {
                        self.status_msg =
                            Some(format!("Filter: {}", self.tree.filter_mode.label()));
                    } else {
                        self.status_msg = Some("Filter cleared".into());
                    }
                }
            }
            KeyCode::Char('m') => {
                let count = self
                    .tree
                    .visible
                    .iter()
                    .filter(|&&idx| {
                        !self.tree.dimmed.contains(&idx) && !self.tree.marked.contains(&idx)
                    })
                    .count();
                if count == 0 {
                    self.status_msg = Some("No items to mark".into());
                } else {
                    self.mark_all_count = count;
                    self.status_msg = Some(format!("Mark {} items? [y/n]", count));
                    self.mode = AppMode::MarkingAll;
                }
            }
            KeyCode::Char('r') => {
                if let Some(idx) = self.tree.selected_node_index() {
                    let path = self.tree.nodes[idx].path.clone();
                    self.tree.nodes[idx].children_loaded = false;
                    // Remove existing children
                    let end = find_subtree_end(&self.tree.nodes, idx);
                    if end > idx + 1 {
                        let to_remove: Vec<usize> = (idx + 1..end).collect();
                        self.tree.remove_nodes(&to_remove);
                    }
                    self.tree.expanded.insert(idx);
                    let _ = self
                        .scan_tx
                        .send(crate::scanner::ScanRequest::ExpandNode(path));
                }
            }
            KeyCode::Char('R') => {
                self.init();
            }
            KeyCode::Char('/') => {
                self.mode = AppMode::Filtering;
                self.filter_text.clear();
            }
            KeyCode::Char('?') => self.mode = AppMode::Help,
            _ => {}
        }
    }

    fn handle_delete_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.perform_delete();
                self.mode = AppMode::Normal;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.delete_candidates.clear();
                self.mode = AppMode::Normal;
            }
            _ => {}
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = AppMode::Normal;
            }
            _ => {}
        }
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.filter_text.clear();
                self.tree.clear_filter();
                self.mode = AppMode::Normal;
            }
            KeyCode::Enter => {
                self.mode = AppMode::Normal;
                // Keep the filter active
            }
            KeyCode::Backspace => {
                self.filter_text.pop();
                self.tree.set_filter(&self.filter_text);
            }
            KeyCode::Char(c) => {
                self.filter_text.push(c);
                self.tree.set_filter(&self.filter_text);
            }
            _ => {}
        }
    }

    fn handle_mark_all_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                for &idx in &self.tree.visible {
                    if !self.tree.dimmed.contains(&idx) {
                        self.tree.marked.insert(idx);
                    }
                }
                self.status_msg = Some(format!("Marked {} items", self.mark_all_count));
                self.mode = AppMode::Normal;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.status_msg = None;
                self.mode = AppMode::Normal;
            }
            _ => {}
        }
    }

    fn perform_delete(&mut self) {
        let mut deleted_count = 0usize;
        let mut freed = 0u64;
        let mut deleted_paths = Vec::new();

        for path in &self.delete_candidates {
            // Measure size before deleting
            let size = crate::scanner::walker::dir_size(path);
            let ok = if path.is_dir() {
                std::fs::remove_dir_all(path).is_ok()
            } else {
                std::fs::remove_file(path).is_ok()
            };
            if ok {
                deleted_count += 1;
                freed += size;
                deleted_paths.push(path.clone());
            }
        }

        if deleted_count > 0 {
            // Remove nodes from tree by matching paths
            let indices: Vec<usize> = deleted_paths
                .iter()
                .filter_map(|p| self.tree.nodes.iter().position(|n| &n.path == p))
                .collect();
            self.tree.remove_nodes(&indices);

            self.status_msg = Some(format!(
                "Deleted {} item{}, freed {}",
                deleted_count,
                if deleted_count == 1 { "" } else { "s" },
                humansize::format_size(freed, humansize::BINARY)
            ));
        }

        self.tree.marked.clear();
        self.delete_candidates.clear();

        // Recompute dimmed set after tree mutation so filter stays consistent
        if self.tree.filter_mode != crate::tree::state::FilterMode::None {
            self.recompute_node_status();
            self.tree.recompute_dimmed(&self.node_status);
            self.tree.snap_selection_to_non_dimmed();
        }
    }

    fn upgrade_command_for_selected(&self) -> Option<String> {
        let node = self.tree.selected_node()?;
        let pkg_name = extract_package_name(&node.name);
        let kind = node.kind;

        // Prefer fix_version from vuln data, fall back to latest from version check
        if let Some(sec) = self.vuln_results.get(&node.path) {
            for vuln in &sec.vulns {
                if let Some(fix) = &vuln.fix_version {
                    return crate::providers::upgrade_command(kind, &pkg_name, fix);
                }
            }
        }
        if let Some(ver) = self.version_results.get(&node.path)
            && ver.is_outdated
        {
            return crate::providers::upgrade_command(kind, &pkg_name, &ver.latest);
        }
        None
    }

    pub fn recompute_node_status(&mut self) {
        self.node_status.clear();

        for path in self.vuln_results.keys() {
            self.node_status.entry(path.clone()).or_default().has_vuln = true;
        }
        for (path, info) in &self.version_results {
            if info.is_outdated {
                self.node_status
                    .entry(path.clone())
                    .or_default()
                    .has_outdated = true;
            }
        }

        // Brew outdated: match formula names to tree node paths
        for node in &self.tree.nodes {
            let pkg_name = extract_package_name(&node.name);
            let from_path = node
                .path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .and_then(|f| {
                    crate::providers::homebrew::parse_bottle_name(&f)
                        .or_else(|| crate::providers::homebrew::parse_manifest_name(&f))
                        .map(|(name, _)| name)
                });
            let name_to_check = from_path.unwrap_or(pkg_name);
            if self.brew_outdated_results.contains_key(&name_to_check) {
                self.node_status
                    .entry(node.path.clone())
                    .or_default()
                    .has_outdated = true;
            }
        }

        // Propagate to all filesystem ancestors so parent folders
        // inherit status even if they're not expanded in the tree
        let affected: Vec<(PathBuf, bool, bool)> = self
            .node_status
            .iter()
            .map(|(p, s)| (p.clone(), s.has_vuln, s.has_outdated))
            .collect();
        for (path, has_vuln, has_outdated) in affected {
            let mut ancestor = path.parent().map(|p| p.to_path_buf());
            while let Some(anc) = ancestor {
                let s = self.node_status.entry(anc.clone()).or_default();
                let changed = (has_vuln && !s.has_vuln) || (has_outdated && !s.has_outdated);
                if has_vuln {
                    s.has_vuln = true;
                }
                if has_outdated {
                    s.has_outdated = true;
                }
                if !changed {
                    break;
                }
                ancestor = anc.parent().map(|p| p.to_path_buf());
            }
        }
    }

    pub fn draw(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(10), // banner
                Constraint::Min(0),     // main area
                Constraint::Length(1),  // bottom bar
            ])
            .split(f.area());

        self.render_banner(f, chunks[0]);
        self.render_main(f, chunks[1]);
        self.render_bottom_bar(f, chunks[2]);

        // Overlays
        match self.mode {
            AppMode::Deleting => {
                let items: Vec<&_> = self
                    .delete_candidates
                    .iter()
                    .filter_map(|p| self.tree.nodes.iter().find(|n| &n.path == p))
                    .collect();
                dialogs::render_delete_confirm(f, &items);
            }
            AppMode::Help => {
                dialogs::render_help(f);
            }
            _ => {}
        }
    }

    fn render_banner(&self, f: &mut Frame, area: Rect) {
        let total_size: u64 = self
            .tree
            .nodes
            .iter()
            .filter(|n| n.parent.is_none())
            .map(|n| n.size)
            .sum();

        let roots_count = self
            .tree
            .nodes
            .iter()
            .filter(|n| n.parent.is_none())
            .count();

        let size_str = if total_size > 0 {
            humansize::format_size(total_size, humansize::BINARY)
        } else {
            "scanning...".to_string()
        };

        let vuln_count = self
            .vuln_results
            .values()
            .map(|s| s.vulns.len())
            .sum::<usize>();
        let outdated_count = self
            .version_results
            .values()
            .filter(|v| v.is_outdated)
            .count();

        let mut stats = format!(
            "{}  │  {} root{}  │  sort: {} {}",
            size_str,
            roots_count,
            if roots_count == 1 { "" } else { "s" },
            self.tree.sort_by.label(),
            if self.tree.sort_desc { "↓" } else { "↑" },
        );
        if self.vulnscan_in_progress {
            stats.push_str("  │  ⚠ scanning...");
        } else if vuln_count > 0 {
            stats.push_str(&format!(
                "  │  ⚠ {} vuln{}",
                vuln_count,
                if vuln_count == 1 { "" } else { "s" }
            ));
        }
        if self.versioncheck_in_progress {
            stats.push_str("  │  ↓ checking...");
        } else if outdated_count > 0 {
            stats.push_str(&format!("  │  ↓ {} outdated", outdated_count));
        }
        if self.tree.filter_mode != crate::tree::state::FilterMode::None {
            stats.push_str(&format!("  │  filter: {}", self.tree.filter_mode.label()));
        }
        stats.push_str("  │  ? help");

        use crate::ui::theme;

        let cyan = ratatui::style::Style::default()
            .fg(ratatui::style::Color::Cyan)
            .add_modifier(ratatui::style::Modifier::BOLD);
        let gold = ratatui::style::Style::default().fg(ratatui::style::Color::Yellow);

        let art: [(&str, &str); 6] = [
            (
                " ██████╗ █████╗  ██████╗██╗  ██╗███████╗",
                " ██████╗ ██████╗ ███╗   ███╗███╗   ███╗ █████╗ ███╗   ██╗██████╗ ███████╗██████╗ ",
            ),
            (
                "██╔════╝██╔══██╗██╔════╝██║  ██║██╔════╝",
                "██╔════╝██╔═══██╗████╗ ████║████╗ ████║██╔══██╗████╗  ██║██╔══██╗██╔════╝██╔══██╗",
            ),
            (
                "██║     ███████║██║     ███████║█████╗  ",
                "██║     ██║   ██║██╔████╔██║██╔████╔██║███████║██╔██╗ ██║██║  ██║█████╗  ██████╔╝",
            ),
            (
                "██║     ██╔══██║██║     ██╔══██║██╔══╝  ",
                "██║     ██║   ██║██║╚██╔╝██║██║╚██╔╝██║██╔══██║██║╚██╗██║██║  ██║██╔══╝  ██╔══██╗",
            ),
            (
                "╚██████╗██║  ██║╚██████╗██║  ██║███████╗",
                "╚██████╗╚██████╔╝██║ ╚═╝ ██║██║ ╚═╝ ██║██║  ██║██║ ╚████║██████╔╝███████╗██║  ██║",
            ),
            (
                " ╚═════╝╚═╝  ╚═╝ ╚═════╝╚═╝  ╚═╝╚══════╝",
                " ╚═════╝ ╚═════╝ ╚═╝     ╚═╝╚═╝     ╚═╝╚═╝  ╚═╝╚═╝  ╚═══╝╚═════╝ ╚══════╝╚═╝  ╚═╝",
            ),
        ];

        // Measure display width using char count (box-drawing chars are multi-byte in UTF-8)
        let art_width = art[0].0.chars().count() + 2 + art[0].1.chars().count();
        let term_width = area.width as usize;
        let pad = if term_width > art_width {
            (term_width - art_width) / 2
        } else {
            0
        };
        let padding = " ".repeat(pad);

        let mut banner_lines: Vec<Line> = vec![Line::from(Span::raw(""))];
        banner_lines.extend(art.iter().map(|(cache, commander)| {
            Line::from(vec![
                Span::raw(&padding),
                Span::styled(*cache, cyan),
                Span::styled("  ", theme::DIM),
                Span::styled(*commander, gold),
            ])
        }));

        banner_lines.push(Line::from(Span::raw("")));

        // Center the stats line too
        let stats_pad = if term_width > stats.len() {
            (term_width - stats.len()) / 2
        } else {
            0
        };
        banner_lines.push(Line::from(vec![
            Span::raw(" ".repeat(stats_pad)),
            Span::styled(&stats, theme::HEADER),
        ]));

        let banner = Paragraph::new(banner_lines)
            .style(ratatui::style::Style::default().bg(ratatui::style::Color::Rgb(15, 15, 26)));
        f.render_widget(banner, area);
    }

    fn render_main(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);

        let viewport_height = chunks[0].height as usize;
        self.tree.adjust_scroll(viewport_height);

        tree_panel::render(f, chunks[0], &self.tree, &self.node_status);
        detail_panel::render(
            f,
            chunks[1],
            &self.tree,
            &self.vuln_results,
            &self.version_results,
            &self.brew_outdated_results,
        );
    }

    fn render_bottom_bar(&self, f: &mut Frame, area: Rect) {
        let marked_count = self.tree.marked.len();
        let marked_hint = if marked_count > 0 {
            format!(" [{marked_count} marked]")
        } else {
            String::new()
        };

        let line = if self.mode == AppMode::Filtering {
            Line::from(vec![
                Span::styled(" /", crate::ui::theme::KEY),
                Span::styled(&self.filter_text, crate::ui::theme::NORMAL),
                Span::styled("█", crate::ui::theme::KEY),
            ])
        } else if let Some(msg) = &self.status_msg {
            Line::from(Span::styled(format!(" {msg}"), crate::ui::theme::SAFE))
        } else {
            Line::from(vec![
                Span::styled(" ↑↓", crate::ui::theme::KEY),
                Span::styled(" navigate  ", crate::ui::theme::NORMAL),
                Span::styled("←→", crate::ui::theme::KEY),
                Span::styled(" expand  ", crate::ui::theme::NORMAL),
                Span::styled("Space", crate::ui::theme::KEY),
                Span::styled(" mark  ", crate::ui::theme::NORMAL),
                Span::styled("d", crate::ui::theme::KEY),
                Span::styled(" delete marked  ", crate::ui::theme::NORMAL),
                Span::styled("s", crate::ui::theme::KEY),
                Span::styled(" sort  ", crate::ui::theme::NORMAL),
                Span::styled("/", crate::ui::theme::KEY),
                Span::styled(" search  ", crate::ui::theme::NORMAL),
                Span::styled("f", crate::ui::theme::KEY),
                Span::styled(" filter  ", crate::ui::theme::NORMAL),
                Span::styled("m", crate::ui::theme::KEY),
                Span::styled(" mark all  ", crate::ui::theme::NORMAL),
                Span::styled(&marked_hint, crate::ui::theme::CAUTION),
            ])
        };

        let bar = Paragraph::new(line)
            .style(ratatui::style::Style::default().bg(ratatui::style::Color::Rgb(30, 30, 50)));
        f.render_widget(bar, area);
    }
}

fn extract_package_name(name: &str) -> String {
    let stripped = if let Some(rest) = name.strip_prefix('[') {
        rest.split_once("] ").map(|(_, n)| n).unwrap_or(name)
    } else {
        name
    };
    stripped
        .split_whitespace()
        .next()
        .unwrap_or(stripped)
        .to_string()
}

fn copy_to_clipboard(text: &str) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // macOS
    if let Ok(mut child) = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        return child.wait().is_ok_and(|s| s.success());
    }

    // Linux (xclip)
    if let Ok(mut child) = Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        return child.wait().is_ok_and(|s| s.success());
    }

    false
}

fn find_subtree_end(nodes: &[crate::tree::node::TreeNode], idx: usize) -> usize {
    let mut end = idx + 1;
    while end < nodes.len() {
        let mut current = end;
        let mut is_descendant = false;
        while let Some(parent) = nodes[current].parent {
            if parent == idx {
                is_descendant = true;
                break;
            }
            current = parent;
        }
        if !is_descendant {
            break;
        }
        end += 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SortField, VersioncheckConfig, VulncheckConfig};
    use crate::providers::homebrew::BrewOutdatedEntry;
    use crate::scanner::{ScanRequest, ScanResult};
    use crate::security::{SecurityInfo, VersionInfo, Vulnerability};
    use crate::tree::node::{CacheKind, TreeNode};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;
    use std::sync::mpsc;

    // --- helpers -----------------------------------------------------------

    fn bare_config() -> Config {
        Config {
            roots: vec![],
            sort_by: SortField::Size,
            sort_desc: true,
            confirm_delete: true,
            vulncheck: VulncheckConfig::default(),
            versioncheck: VersioncheckConfig::default(),
        }
    }

    /// Build an App wired to two *local* channels so tests can push
    /// ScanResults into it (`result_tx`) and inspect the ScanRequests it
    /// sends (`scan_rx`). No background scanner thread is started.
    fn build_app(config: Config) -> (App, mpsc::Sender<ScanResult>, mpsc::Receiver<ScanRequest>) {
        let (result_tx, result_rx) = mpsc::channel::<ScanResult>();
        let (scan_tx, scan_rx) = mpsc::channel::<ScanRequest>();
        let app = App::new(config, result_rx, scan_tx);
        (app, result_tx, scan_rx)
    }

    fn mk_node(name: &str, size: u64, kind: CacheKind) -> TreeNode {
        let mut n = TreeNode::new(PathBuf::from(format!("/tmp/{name}")), 0, None);
        n.name = name.into();
        n.kind = kind;
        n.size = size;
        n.has_children = false;
        n.children_loaded = true;
        n
    }

    fn mk_node_with_path(name: &str, path: PathBuf, kind: CacheKind) -> TreeNode {
        let mut n = TreeNode::new(path, 0, None);
        n.name = name.into();
        n.kind = kind;
        n.size = 1024;
        n.has_children = false;
        n.children_loaded = true;
        n
    }

    fn render_app(app: &mut App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
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

    // --- pure helpers -----------------------------------------------------

    #[test]
    fn extract_package_name_variants() {
        assert_eq!(extract_package_name("serde"), "serde");
        assert_eq!(extract_package_name("serde 1.0.200"), "serde");
        assert_eq!(extract_package_name("[PyPI] requests 2.31.0"), "requests");
        assert_eq!(extract_package_name("[broken"), "[broken");
        assert_eq!(extract_package_name(""), "");
    }

    #[test]
    fn find_subtree_end_linear() {
        // Flat layout: root, child0, child1, sibling
        let mut nodes = vec![
            TreeNode::new(PathBuf::from("/r"), 0, None),
            TreeNode::new(PathBuf::from("/r/a"), 1, Some(0)),
            TreeNode::new(PathBuf::from("/r/b"), 1, Some(0)),
            TreeNode::new(PathBuf::from("/r2"), 0, None),
        ];
        for n in &mut nodes {
            n.has_children = false;
        }
        assert_eq!(find_subtree_end(&nodes, 0), 3);
        assert_eq!(find_subtree_end(&nodes, 1), 2);
        assert_eq!(find_subtree_end(&nodes, 3), 4);
    }

    #[test]
    fn find_subtree_end_nested() {
        // root(0) → a(1) → a.x(2) → a.x.y(3), sibling b(4)
        let mut nodes = vec![
            TreeNode::new(PathBuf::from("/r"), 0, None),
            TreeNode::new(PathBuf::from("/r/a"), 1, Some(0)),
            TreeNode::new(PathBuf::from("/r/a/x"), 2, Some(1)),
            TreeNode::new(PathBuf::from("/r/a/x/y"), 3, Some(2)),
            TreeNode::new(PathBuf::from("/r/b"), 1, Some(0)),
        ];
        for n in &mut nodes {
            n.has_children = false;
        }
        assert_eq!(find_subtree_end(&nodes, 0), 5);
        assert_eq!(find_subtree_end(&nodes, 1), 4);
        assert_eq!(find_subtree_end(&nodes, 2), 4);
    }

    // --- App::draw render tests -------------------------------------------

    #[test]
    fn draw_renders_banner_with_size_and_root_count() {
        let (mut app, _tx, _rx) = build_app(bare_config());
        app.tree.set_roots(vec![
            mk_node("alpha", 5 * 1024 * 1024, CacheKind::Cargo),
            mk_node("beta", 3 * 1024 * 1024, CacheKind::Npm),
        ]);
        let out = render_app(&mut app, 140, 30);
        assert!(out.contains("8 MiB"), "total size:\n{out}");
        assert!(out.contains("2 roots"), "root count:\n{out}");
        assert!(out.contains("sort:"), "sort label:\n{out}");
        assert!(out.contains("help"), "help hint:\n{out}");
        assert!(out.contains("alpha"), "tree row alpha:\n{out}");
        assert!(out.contains("beta"));
    }

    #[test]
    fn draw_banner_shows_scanning_when_no_sizes_yet() {
        let (mut app, _tx, _rx) = build_app(bare_config());
        app.tree
            .set_roots(vec![mk_node("pending", 0, CacheKind::Cargo)]);
        let out = render_app(&mut app, 140, 30);
        assert!(out.contains("scanning..."), "scanning placeholder:\n{out}");
        assert!(out.contains("1 root"), "singular root:\n{out}");
    }

    #[test]
    fn draw_banner_shows_vuln_and_outdated_counters() {
        let (mut app, _tx, _rx) = build_app(bare_config());
        let node = mk_node("urllib3", 1024, CacheKind::Pip);
        let path = node.path.clone();
        app.tree.set_roots(vec![node]);
        app.vuln_results.insert(
            path.clone(),
            SecurityInfo {
                vulns: vec![Vulnerability {
                    id: "CVE-1".into(),
                    summary: "".into(),
                    severity: None,
                    fix_version: None,
                }],
            },
        );
        app.version_results.insert(
            path,
            VersionInfo {
                current: "1.0".into(),
                latest: "2.0".into(),
                is_outdated: true,
            },
        );
        let out = render_app(&mut app, 160, 30);
        assert!(out.contains("1 vuln"), "vuln counter:\n{out}");
        assert!(out.contains("1 outdated"), "outdated counter:\n{out}");
    }

    #[test]
    fn draw_banner_shows_in_progress_indicators() {
        let (mut app, _tx, _rx) = build_app(bare_config());
        app.tree.set_roots(vec![mk_node("x", 1, CacheKind::Cargo)]);
        app.vulnscan_in_progress = true;
        app.versioncheck_in_progress = true;
        let out = render_app(&mut app, 160, 30);
        assert!(out.contains("scanning..."), "vuln scanning:\n{out}");
        assert!(out.contains("checking..."), "version checking:\n{out}");
    }

    #[test]
    fn draw_renders_help_overlay_in_help_mode() {
        let (mut app, _tx, _rx) = build_app(bare_config());
        app.tree.set_roots(vec![mk_node("a", 1, CacheKind::Cargo)]);
        app.mode = AppMode::Help;
        let out = render_app(&mut app, 140, 60);
        assert!(out.contains("Help"), "help title:\n{out}");
        assert!(out.contains("Move up"), "help content:\n{out}");
    }

    #[test]
    fn draw_renders_delete_confirm_in_deleting_mode() {
        let (mut app, _tx, _rx) = build_app(bare_config());
        let node = mk_node("doomed", 4096, CacheKind::Cargo);
        app.delete_candidates = vec![node.path.clone()];
        app.tree.set_roots(vec![node]);
        app.mode = AppMode::Deleting;
        let out = render_app(&mut app, 140, 40);
        assert!(out.contains("Delete 1 item"), "confirm dialog:\n{out}");
        assert!(out.contains("doomed"));
    }

    #[test]
    fn draw_bottom_bar_shows_filter_input_in_filtering_mode() {
        let (mut app, _tx, _rx) = build_app(bare_config());
        app.tree.set_roots(vec![mk_node("a", 1, CacheKind::Cargo)]);
        app.mode = AppMode::Filtering;
        app.filter_text = "serde".into();
        let out = render_app(&mut app, 140, 20);
        assert!(out.contains("/"), "filter prompt:\n{out}");
        assert!(out.contains("serde"), "filter text:\n{out}");
    }

    #[test]
    fn draw_bottom_bar_shows_status_message_when_set() {
        let (mut app, _tx, _rx) = build_app(bare_config());
        app.tree.set_roots(vec![mk_node("a", 1, CacheKind::Cargo)]);
        app.status_msg = Some("Scanned 42 packages".into());
        let out = render_app(&mut app, 140, 20);
        assert!(out.contains("Scanned 42 packages"), "status msg:\n{out}");
    }

    #[test]
    fn draw_bottom_bar_default_shows_hotkey_hints() {
        let (mut app, _tx, _rx) = build_app(bare_config());
        app.tree.set_roots(vec![mk_node("a", 1, CacheKind::Cargo)]);
        let out = render_app(&mut app, 140, 20);
        for h in &["navigate", "expand", "mark", "delete", "sort"] {
            assert!(out.contains(h), "missing hint '{h}':\n{out}");
        }
    }

    // --- tick / ScanResult handling ---------------------------------------

    #[test]
    fn tick_roots_scanned_replaces_tree() {
        let (mut app, tx, _rx) = build_app(bare_config());
        tx.send(ScanResult::RootsScanned(vec![mk_node(
            "r",
            100,
            CacheKind::Cargo,
        )]))
        .unwrap();
        app.tick();
        assert_eq!(app.tree.nodes.len(), 1);
        assert_eq!(app.tree.nodes[0].name, "r");
    }

    #[test]
    fn tick_size_updated_mutates_existing_node() {
        let (mut app, tx, _rx) = build_app(bare_config());
        let node = mk_node("r", 0, CacheKind::Cargo);
        let path = node.path.clone();
        app.tree.set_roots(vec![node]);
        tx.send(ScanResult::SizeUpdated(path, 9999)).unwrap();
        app.tick();
        assert_eq!(app.tree.nodes[0].size, 9999);
    }

    #[test]
    fn tick_vulns_scanned_merges_and_sets_status_singular() {
        let (mut app, tx, _rx) = build_app(bare_config());
        let node = mk_node("urllib3", 1, CacheKind::Pip);
        let path = node.path.clone();
        app.tree.set_roots(vec![node]);
        let mut results = HashMap::new();
        results.insert(
            path.clone(),
            SecurityInfo {
                vulns: vec![Vulnerability {
                    id: "CVE-1".into(),
                    summary: "".into(),
                    severity: None,
                    fix_version: None,
                }],
            },
        );
        tx.send(ScanResult::VulnsScanned(1, results)).unwrap();
        app.tick();
        assert!(app.vuln_results.contains_key(&path));
        assert!(!app.vulnscan_in_progress);
        let msg = app.status_msg.as_deref().unwrap_or("");
        assert!(msg.contains("1 vulnerability"), "singular grammar: {msg}");
        assert!(app.node_status.get(&path).unwrap().has_vuln);
    }

    #[test]
    fn tick_vulns_scanned_zero_uses_clean_message() {
        let (mut app, tx, _rx) = build_app(bare_config());
        app.tree.set_roots(vec![mk_node("ok", 1, CacheKind::Cargo)]);
        tx.send(ScanResult::VulnsScanned(5, HashMap::new()))
            .unwrap();
        app.tick();
        let msg = app.status_msg.as_deref().unwrap_or("");
        assert!(msg.contains("no vulnerabilities"), "clean msg: {msg}");
    }

    #[test]
    fn tick_versions_checked_sets_status_and_outdated_flag() {
        let (mut app, tx, _rx) = build_app(bare_config());
        let node = mk_node("serde", 1, CacheKind::Cargo);
        let path = node.path.clone();
        app.tree.set_roots(vec![node]);
        let mut results = HashMap::new();
        results.insert(
            path.clone(),
            VersionInfo {
                current: "1.0.100".into(),
                latest: "1.0.200".into(),
                is_outdated: true,
            },
        );
        tx.send(ScanResult::VersionsChecked(1, results)).unwrap();
        app.tick();
        assert!(!app.versioncheck_in_progress);
        assert_eq!(app.version_results.len(), 1);
        let msg = app.status_msg.as_deref().unwrap_or("");
        assert!(msg.contains("1 outdated"), "outdated msg: {msg}");
        assert!(app.node_status.get(&path).unwrap().has_outdated);
    }

    #[test]
    fn tick_versions_checked_all_up_to_date_message() {
        let (mut app, tx, _rx) = build_app(bare_config());
        app.tree
            .set_roots(vec![mk_node("serde", 1, CacheKind::Cargo)]);
        tx.send(ScanResult::VersionsChecked(3, HashMap::new()))
            .unwrap();
        app.tick();
        let msg = app.status_msg.as_deref().unwrap_or("");
        assert!(msg.contains("all up to date"), "clean msg: {msg}");
    }

    #[test]
    fn tick_brew_outdated_sets_status_and_nodestate() {
        let (mut app, tx, _rx) = build_app(bare_config());
        // Node name must match how brew matching works (extract_package_name on name).
        app.tree
            .set_roots(vec![mk_node("wget", 1, CacheKind::Homebrew)]);
        let mut results = HashMap::new();
        results.insert(
            "wget".to_string(),
            BrewOutdatedEntry {
                installed: "1.21".into(),
                current: "1.24".into(),
                pinned: false,
            },
        );
        tx.send(ScanResult::BrewOutdatedCompleted(results)).unwrap();
        app.tick();
        assert!(!app.brew_outdated_in_progress);
        assert_eq!(app.brew_outdated_results.len(), 1);
        let msg = app.status_msg.as_deref().unwrap_or("");
        assert!(msg.contains("1 outdated package"), "brew msg: {msg}");
    }

    #[test]
    fn tick_brew_outdated_zero_does_not_set_status() {
        let (mut app, tx, _rx) = build_app(bare_config());
        app.tree
            .set_roots(vec![mk_node("x", 1, CacheKind::Homebrew)]);
        tx.send(ScanResult::BrewOutdatedCompleted(HashMap::new()))
            .unwrap();
        app.tick();
        assert!(app.status_msg.is_none(), "should stay silent on zero");
    }

    #[test]
    fn tick_children_scanned_inserts_into_matching_parent() {
        let (mut app, tx, _rx) = build_app(bare_config());
        let mut parent = mk_node("root", 0, CacheKind::Cargo);
        parent.has_children = true;
        parent.children_loaded = false;
        let parent_path = parent.path.clone();
        app.tree.set_roots(vec![parent]);
        let child = {
            let mut c = TreeNode::new(parent_path.join("child"), 1, Some(0));
            c.name = "child".into();
            c.has_children = false;
            c
        };
        tx.send(ScanResult::ChildrenScanned(parent_path, vec![child]))
            .unwrap();
        app.tick();
        assert!(app.tree.nodes.iter().any(|n| n.name == "child"));
    }

    #[test]
    fn tick_triggers_auto_vuln_and_version_when_enabled() {
        let mut cfg = bare_config();
        cfg.vulncheck.enabled = true;
        cfg.versioncheck.enabled = true;
        let (mut app, _tx, rx) = build_app(cfg);
        app.tree.set_roots(vec![mk_node("r", 1, CacheKind::Cargo)]);
        app.tick();
        // The auto flags should have flipped to "in progress" and fired requests.
        assert!(app.vulnscan_in_progress);
        assert!(app.versioncheck_in_progress);
        let mut saw_vuln = false;
        let mut saw_ver = false;
        while let Ok(req) = rx.try_recv() {
            match req {
                ScanRequest::ScanVulns(_) => saw_vuln = true,
                ScanRequest::CheckVersions(_) => saw_ver = true,
                _ => {}
            }
        }
        assert!(saw_vuln && saw_ver, "both auto requests sent");
    }

    // --- perform_delete ---------------------------------------------------

    #[test]
    fn perform_delete_removes_real_files_and_updates_status() {
        let tmp = tempfile::tempdir().unwrap();
        let file_a = tmp.path().join("a.txt");
        let file_b = tmp.path().join("b.txt");
        std::fs::write(&file_a, b"aaaa").unwrap();
        std::fs::write(&file_b, b"bbbb").unwrap();

        let (mut app, _tx, _rx) = build_app(bare_config());
        let node_a = mk_node_with_path("a", file_a.clone(), CacheKind::Cargo);
        let node_b = mk_node_with_path("b", file_b.clone(), CacheKind::Cargo);
        app.tree.set_roots(vec![node_a, node_b]);
        app.delete_candidates = vec![file_a.clone(), file_b.clone()];

        app.perform_delete();

        assert!(!file_a.exists(), "file_a should be gone");
        assert!(!file_b.exists(), "file_b should be gone");
        let msg = app.status_msg.as_deref().unwrap_or("");
        assert!(msg.contains("Deleted 2"), "deleted status: {msg}");
        assert!(app.delete_candidates.is_empty());
    }

    #[test]
    fn perform_delete_removes_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("cachedir");
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("nested/x"), b"xx").unwrap();

        let (mut app, _tx, _rx) = build_app(bare_config());
        let node = mk_node_with_path("cachedir", dir.clone(), CacheKind::Cargo);
        app.tree.set_roots(vec![node]);
        app.delete_candidates = vec![dir.clone()];

        app.perform_delete();

        assert!(!dir.exists(), "directory should be removed");
        assert!(
            app.status_msg
                .as_deref()
                .unwrap()
                .contains("Deleted 1 item")
        );
    }

    #[test]
    fn perform_delete_no_op_on_empty_candidates_leaves_status_unchanged() {
        let (mut app, _tx, _rx) = build_app(bare_config());
        app.perform_delete();
        assert!(app.status_msg.is_none());
    }

    // --- upgrade_command_for_selected -------------------------------------

    #[test]
    fn upgrade_command_prefers_vuln_fix_version() {
        let (mut app, _tx, _rx) = build_app(bare_config());
        let node = mk_node("urllib3 1.26.5", 1, CacheKind::Pip);
        let path = node.path.clone();
        app.tree.set_roots(vec![node]);
        app.vuln_results.insert(
            path.clone(),
            SecurityInfo {
                vulns: vec![Vulnerability {
                    id: "CVE".into(),
                    summary: "".into(),
                    severity: None,
                    fix_version: Some("1.26.17".into()),
                }],
            },
        );
        // version info points at an even newer one — vuln fix should still win.
        app.version_results.insert(
            path,
            VersionInfo {
                current: "1.26.5".into(),
                latest: "2.0.0".into(),
                is_outdated: true,
            },
        );
        let cmd = app.upgrade_command_for_selected().expect("cmd");
        assert!(cmd.contains("urllib3"));
        assert!(
            cmd.contains("1.26.17"),
            "uses fix version, not latest: {cmd}"
        );
    }

    #[test]
    fn upgrade_command_falls_back_to_latest_when_no_vuln() {
        // Use Pip because its upgrade_command template includes the version —
        // Cargo's is `cargo update -p <name>` with no version, which wouldn't
        // distinguish "uses latest" from "uses anything".
        let (mut app, _tx, _rx) = build_app(bare_config());
        let node = mk_node("requests 2.20.0", 1, CacheKind::Pip);
        let path = node.path.clone();
        app.tree.set_roots(vec![node]);
        app.version_results.insert(
            path,
            VersionInfo {
                current: "2.20.0".into(),
                latest: "2.31.0".into(),
                is_outdated: true,
            },
        );
        let cmd = app.upgrade_command_for_selected().expect("cmd");
        assert!(cmd.contains("requests"), "pkg name: {cmd}");
        assert!(cmd.contains("2.31.0"), "latest version: {cmd}");
    }

    #[test]
    fn upgrade_command_none_when_clean() {
        let (mut app, _tx, _rx) = build_app(bare_config());
        app.tree
            .set_roots(vec![mk_node("clean", 1, CacheKind::Cargo)]);
        assert!(app.upgrade_command_for_selected().is_none());
    }

    // --- recompute_node_status --------------------------------------------

    #[test]
    fn recompute_node_status_propagates_to_ancestors() {
        let (mut app, _tx, _rx) = build_app(bare_config());
        // Build a 3-level tree: /r → /r/sub → /r/sub/pkg
        let mut root = TreeNode::new(PathBuf::from("/r"), 0, None);
        root.name = "r".into();
        root.has_children = true;
        let mut sub = TreeNode::new(PathBuf::from("/r/sub"), 1, Some(0));
        sub.name = "sub".into();
        let mut pkg = TreeNode::new(PathBuf::from("/r/sub/pkg"), 2, Some(1));
        pkg.name = "pkg".into();

        app.tree.nodes = vec![root, sub, pkg];
        app.vuln_results.insert(
            PathBuf::from("/r/sub/pkg"),
            SecurityInfo {
                vulns: vec![Vulnerability {
                    id: "CVE".into(),
                    summary: "".into(),
                    severity: None,
                    fix_version: None,
                }],
            },
        );

        app.recompute_node_status();

        // Ancestor propagation: /r/sub, /r, and / should all inherit has_vuln.
        assert!(
            app.node_status
                .get(&PathBuf::from("/r/sub/pkg"))
                .unwrap()
                .has_vuln
        );
        assert!(
            app.node_status
                .get(&PathBuf::from("/r/sub"))
                .unwrap()
                .has_vuln
        );
        assert!(app.node_status.get(&PathBuf::from("/r")).unwrap().has_vuln);
    }

    #[test]
    fn recompute_node_status_matches_brew_by_semantic_name() {
        let (mut app, _tx, _rx) = build_app(bare_config());
        let node = mk_node("wget 1.21", 1, CacheKind::Homebrew);
        let path = node.path.clone();
        app.tree.set_roots(vec![node]);
        app.brew_outdated_results.insert(
            "wget".into(),
            BrewOutdatedEntry {
                installed: "1.21".into(),
                current: "1.24".into(),
                pinned: false,
            },
        );

        app.recompute_node_status();

        assert!(
            app.node_status.get(&path).unwrap().has_outdated,
            "wget node should be flagged outdated via brew match"
        );
    }

    #[test]
    fn recompute_node_status_clears_previous() {
        let (mut app, _tx, _rx) = build_app(bare_config());
        app.tree.set_roots(vec![mk_node("x", 1, CacheKind::Cargo)]);
        app.node_status.insert(
            PathBuf::from("/stale/path"),
            crate::security::NodeStatus {
                has_vuln: true,
                has_outdated: true,
            },
        );
        app.recompute_node_status();
        assert!(
            !app.node_status.contains_key(&PathBuf::from("/stale/path")),
            "stale entries must be cleared"
        );
    }
}
