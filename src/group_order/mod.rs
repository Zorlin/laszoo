use std::path::PathBuf;
use std::fs;
use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    Frame,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io;
use std::time::Duration;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupNode {
    pub name: String,
    pub children: Vec<GroupNode>,
    #[serde(skip)]
    pub expanded: bool,
    #[serde(skip)]
    pub selected: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GroupOrder {
    pub version: u32,
    pub groups: Vec<GroupNode>,
}

impl Default for GroupOrder {
    fn default() -> Self {
        Self {
            version: 1,
            groups: Vec::new(),
        }
    }
}

pub struct GroupOrderTui {
    mount_point: PathBuf,
    order: GroupOrder,
    selected_index: usize,
    flat_view: Vec<(String, usize)>, // (name, depth)
    move_mode: bool, // True when holding shift
}

impl GroupOrderTui {
    pub fn new(mount_point: PathBuf) -> Result<Self> {
        // Get all groups from filesystem FIRST
        let groups_dir = mount_point.join("groups");
        let mut all_groups = Vec::new();
        if groups_dir.exists() {
            for entry in fs::read_dir(&groups_dir)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().to_string();
                if entry.file_type()?.is_dir() && !name.starts_with('.') {
                    all_groups.push(name);
                }
            }
        }
        all_groups.sort();

        // Load existing order or create new with ALL groups
        let order_path = mount_point.join("groups/order.json");
        let mut order = if order_path.exists() {
            let content = fs::read_to_string(&order_path)?;
            let mut loaded_order: GroupOrder = serde_json::from_str(&content)?;
            
            // Add any missing groups to the hierarchy
            let existing_groups: HashSet<String> = loaded_order.groups.iter()
                .map(|n| n.name.clone())
                .collect();
            
            for group in &all_groups {
                if !existing_groups.contains(group) {
                    loaded_order.groups.push(GroupNode {
                        name: group.clone(),
                        children: Vec::new(),
                        expanded: false,
                        selected: false,
                    });
                }
            }
            
            loaded_order
        } else {
            // Create new order with all groups
            let groups = all_groups.iter().map(|name| GroupNode {
                name: name.clone(),
                children: Vec::new(),
                expanded: false,
                selected: false,
            }).collect();
            
            GroupOrder {
                version: 1,
                groups,
            }
        };

        let mut tui = Self {
            mount_point,
            order,
            selected_index: 0,
            flat_view: Vec::new(),
            move_mode: false,
        };
        
        tui.rebuild_flat_view();
        Ok(tui)
    }

    fn rebuild_flat_view(&mut self) {
        self.flat_view.clear();
        
        // Just add all groups from the hierarchy - they're all there now
        let groups = self.order.groups.clone();
        for node in &groups {
            self.add_node_to_flat_view(node, 0);
        }
    }

    fn add_node_to_flat_view(&mut self, node: &GroupNode, depth: usize) {
        self.flat_view.push((node.name.clone(), depth));
        if node.expanded {
            for child in &node.children {
                self.add_node_to_flat_view(child, depth + 1);
            }
        }
    }


    fn find_node_in_hierarchy<'a>(&self, name: &str, nodes: &'a [GroupNode]) -> Option<&'a GroupNode> {
        for node in nodes {
            if node.name == name {
                return Some(node);
            }
            if let Some(found) = self.find_node_in_hierarchy(name, &node.children) {
                return Some(found);
            }
        }
        None
    }

    fn find_node_mut_in_vec<'a>(name: &str, nodes: &'a mut Vec<GroupNode>) -> Option<&'a mut GroupNode> {
        for node in nodes.iter_mut() {
            if node.name == name {
                return Some(node);
            }
            if let Some(found) = Self::find_node_mut_in_vec(name, &mut node.children) {
                return Some(found);
            }
        }
        None
    }

    pub async fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let res = self.run_app(&mut terminal).await;

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        if let Err(err) = res {
            println!("{:?}", err)
        }

        Ok(())
    }

    async fn run_app<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        loop {
            terminal.draw(|f| self.ui(f))?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match self.handle_key(key) {
                        Ok(should_quit) => {
                            if should_quit {
                                return Ok(());
                            }
                        }
                        Err(e) => return Err(e),
                    }
                }
            }
        }
    }

    fn ui(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(4),
            ])
            .split(f.area());

        // Title
        let title = Paragraph::new("Group Hierarchy and Variable Precedence Order")
            .style(Style::default().fg(Color::Cyan))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(title, chunks[0]);

        // Group list
        let items: Vec<ListItem> = self.flat_view.iter().enumerate().map(|(idx, (name, depth))| {
            let indent = "  ".repeat(*depth);
            let prefix = if self.has_children(name) {
                if self.is_expanded(name) { "▼ " } else { "▶ " }
            } else {
                "  "
            };
            
            let style = if self.move_mode && self.is_selected(name) {
                // Selected items in move mode get special styling
                if idx == self.selected_index {
                    Style::default().bg(Color::Magenta).fg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
                }
            } else if idx == self.selected_index {
                if self.is_selected(name) {
                    Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().bg(Color::DarkGray).fg(Color::White)
                }
            } else if self.is_selected(name) {
                Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            
            ListItem::new(format!("{}{}{}", indent, prefix, name)).style(style)
        }).collect();

        let groups_list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Groups"))
            .style(Style::default().fg(Color::White));
        f.render_widget(groups_list, chunks[1]);

        // Help text
        let help_text = vec![
            Line::from(vec![
                Span::raw("↑/↓: Navigate  "),
                Span::raw("Space: Select/Unselect  "),
                Span::raw("Tab/S-Tab: Indent  "),
            ]),
            Line::from(vec![
                Span::raw("Hold Shift: Grab  "),
                Span::raw("Shift+↑/↓: Move grabbed  "),
                Span::raw("s: Save  "),
                Span::raw("q/Esc: Quit"),
            ]),
        ];
        let help = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL).title("Controls"));
        f.render_widget(help, chunks[2]);
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        // Check if Shift is being held to enter/maintain grab mode
        self.move_mode = key.modifiers.contains(KeyModifiers::SHIFT);
        
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
            KeyCode::Char('s') => {
                self.save()?;
                return Ok(false);
            }
            KeyCode::Up => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Shift+Up: Move selected items up
                    self.move_selected_up()?;
                } else {
                    // Just Up: Navigate
                    if self.selected_index > 0 {
                        self.selected_index -= 1;
                    }
                }
            }
            KeyCode::Down => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Shift+Down: Move selected items down
                    self.move_selected_down()?;
                } else {
                    // Just Down: Navigate
                    if self.selected_index < self.flat_view.len().saturating_sub(1) {
                        self.selected_index += 1;
                    }
                }
            }
            KeyCode::Char(' ') => {
                // Space: Toggle selection only
                self.toggle_selection();
            }
            KeyCode::Enter => {
                self.toggle_expansion();
            }
            KeyCode::Tab => {
                self.indent()?;
            }
            KeyCode::BackTab => {
                self.unindent()?;
            }
            _ => {}
        }
        Ok(false)
    }

    fn toggle_selection(&mut self) {
        if let Some((name, _)) = self.flat_view.get(self.selected_index).cloned() {
            // Find the node in hierarchy and toggle its selection
            if let Some(node) = Self::find_node_mut_in_vec(&name, &mut self.order.groups) {
                node.selected = !node.selected;
            }
            // Don't rebuild flat view - selection doesn't change structure
        }
    }

    fn toggle_expansion(&mut self) {
        if let Some((name, _)) = self.flat_view.get(self.selected_index).cloned() {
            if let Some(node) = Self::find_node_mut_in_vec(&name, &mut self.order.groups) {
                if !node.children.is_empty() {
                    node.expanded = !node.expanded;
                    self.rebuild_flat_view();
                }
            }
        }
    }

    fn has_children(&self, name: &str) -> bool {
        if let Some(node) = self.find_node_in_hierarchy(name, &self.order.groups) {
            !node.children.is_empty()
        } else {
            false
        }
    }

    fn is_expanded(&self, name: &str) -> bool {
        if let Some(node) = self.find_node_in_hierarchy(name, &self.order.groups) {
            node.expanded
        } else {
            false
        }
    }

    fn is_selected(&self, name: &str) -> bool {
        if let Some(node) = self.find_node_in_hierarchy(name, &self.order.groups) {
            node.selected
        } else {
            false
        }
    }

    fn indent(&mut self) -> Result<()> {
        if self.selected_index == 0 {
            return Ok(()); // Can't indent the first item
        }
        
        if let Some((name, current_depth)) = self.flat_view.get(self.selected_index).cloned() {
            // Find the previous item at the same or lower depth level to make it parent
            let mut potential_parent_idx = None;
            for i in (0..self.selected_index).rev() {
                if let Some((_, depth)) = self.flat_view.get(i) {
                    if *depth == current_depth {
                        potential_parent_idx = Some(i);
                        break;
                    } else if *depth < current_depth {
                        break; // Stop if we find something at a lower depth
                    }
                }
            }
            
            if let Some(parent_idx) = potential_parent_idx {
                if let Some((parent_name, _)) = self.flat_view.get(parent_idx).cloned() {
                    // Remove from current location and add as child of parent
                    self.reparent_node(&name, Some(&parent_name))?;
                    self.rebuild_flat_view();
                }
            }
        }
        Ok(())
    }

    fn unindent(&mut self) -> Result<()> {
        if let Some((name, depth)) = self.flat_view.get(self.selected_index).cloned() {
            if depth > 0 {
                // Find current parent and move to its level
                self.reparent_node(&name, None)?;
                self.rebuild_flat_view();
            }
        }
        Ok(())
    }

    fn reparent_node(&mut self, node_name: &str, new_parent_name: Option<&str>) -> Result<()> {
        // First, find and remove the node from its current location
        let node = self.remove_node_from_hierarchy(node_name)?;
        
        // Then add it to the new location
        if let Some(parent_name) = new_parent_name {
            // Add as child of the specified parent
            if let Some(parent) = Self::find_node_mut_in_vec(parent_name, &mut self.order.groups) {
                parent.children.push(node);
                parent.expanded = true; // Expand parent to show new child
            } else {
                return Err(anyhow!("Parent node not found"));
            }
        } else {
            // Add to root level
            self.order.groups.push(node);
        }
        
        Ok(())
    }

    fn remove_node_from_hierarchy(&mut self, name: &str) -> Result<GroupNode> {
        // Try to remove from root level
        if let Some(pos) = self.order.groups.iter().position(|n| n.name == name) {
            return Ok(self.order.groups.remove(pos));
        }
        
        // Otherwise search in children
        fn remove_from_children(name: &str, nodes: &mut Vec<GroupNode>) -> Option<GroupNode> {
            for node in nodes.iter_mut() {
                if let Some(pos) = node.children.iter().position(|n| n.name == name) {
                    return Some(node.children.remove(pos));
                }
                if let Some(found) = remove_from_children(name, &mut node.children) {
                    return Some(found);
                }
            }
            None
        }
        
        remove_from_children(name, &mut self.order.groups)
            .ok_or_else(|| anyhow!("Node not found in hierarchy"))
    }

    fn move_up(&mut self) -> Result<()> {
        if self.selected_index == 0 {
            return Ok(());
        }
        
        if let Some((name, depth)) = self.flat_view.get(self.selected_index).cloned() {
            // Find the previous sibling at the same depth
            for i in (0..self.selected_index).rev() {
                if let Some((_, prev_depth)) = self.flat_view.get(i) {
                    if *prev_depth == depth {
                        // Found a sibling, swap them
                        self.swap_siblings(&name, i)?;
                        self.rebuild_flat_view();
                        self.selected_index = i;
                        break;
                    } else if *prev_depth < depth {
                        break; // No more siblings at this level
                    }
                }
            }
        }
        Ok(())
    }

    fn move_down(&mut self) -> Result<()> {
        if self.selected_index >= self.flat_view.len() - 1 {
            return Ok(());
        }
        
        if let Some((name, depth)) = self.flat_view.get(self.selected_index).cloned() {
            // Find the next sibling at the same depth
            for i in (self.selected_index + 1)..self.flat_view.len() {
                if let Some((_, next_depth)) = self.flat_view.get(i) {
                    if *next_depth == depth {
                        // Found a sibling, swap them
                        self.swap_siblings(&name, i)?;
                        self.rebuild_flat_view();
                        
                        // Find new position
                        for (idx, (n, _)) in self.flat_view.iter().enumerate() {
                            if n == &name {
                                self.selected_index = idx;
                                break;
                            }
                        }
                        break;
                    } else if *next_depth < depth {
                        break; // No more siblings at this level
                    }
                }
            }
        }
        Ok(())
    }
    
    fn move_selected_up(&mut self) -> Result<()> {
        // Get all selected items in order
        let mut selected_items: Vec<(String, usize)> = Vec::new();
        for (name, depth) in &self.flat_view {
            if self.is_selected(name) {
                selected_items.push((name.clone(), *depth));
            }
        }
        
        // Move each selected item up
        for (name, _) in selected_items {
            // Find current position
            if let Some(pos) = self.flat_view.iter().position(|(n, _)| n == &name) {
                if pos > 0 {
                    // Store old index, do the move
                    let old_idx = self.selected_index;
                    self.selected_index = pos;
                    self.move_up()?;
                    self.selected_index = old_idx;
                }
            }
        }
        
        Ok(())
    }
    
    fn move_selected_down(&mut self) -> Result<()> {
        // Get all selected items in reverse order (to move bottom items first)
        let mut selected_items: Vec<(String, usize)> = Vec::new();
        for (name, depth) in &self.flat_view {
            if self.is_selected(name) {
                selected_items.push((name.clone(), *depth));
            }
        }
        selected_items.reverse();
        
        // Move each selected item down
        for (name, _) in selected_items {
            // Find current position
            if let Some(pos) = self.flat_view.iter().position(|(n, _)| n == &name) {
                if pos < self.flat_view.len() - 1 {
                    // Store old index, do the move
                    let old_idx = self.selected_index;
                    self.selected_index = pos;
                    self.move_down()?;
                    self.selected_index = old_idx;
                }
            }
        }
        
        Ok(())
    }

    fn swap_siblings(&mut self, node_name: &str, sibling_flat_idx: usize) -> Result<()> {
        if let Some((sibling_name, _)) = self.flat_view.get(sibling_flat_idx).cloned() {
            // Find parent containing both nodes
            fn find_and_swap(name1: &str, name2: &str, nodes: &mut Vec<GroupNode>) -> bool {
                // Check if both are at this level
                let pos1 = nodes.iter().position(|n| n.name == name1);
                let pos2 = nodes.iter().position(|n| n.name == name2);
                
                if let (Some(p1), Some(p2)) = (pos1, pos2) {
                    nodes.swap(p1, p2);
                    return true;
                }
                
                // Otherwise check children
                for node in nodes.iter_mut() {
                    if find_and_swap(name1, name2, &mut node.children) {
                        return true;
                    }
                }
                
                false
            }
            
            find_and_swap(node_name, &sibling_name, &mut self.order.groups);
        }
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let order_path = self.mount_point.join("groups/order.json");
        let content = serde_json::to_string_pretty(&self.order)?;
        fs::write(&order_path, content)?;
        Ok(())
    }
}