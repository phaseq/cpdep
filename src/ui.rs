use crate::graph::{ComponentRef, Edge, Graph};
use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::collections::HashMap;
use std::io::{stdout, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tui::backend::CrosstermBackend;
use tui::layout::{Constraint, Direction, Layout};
use tui::style::{Color, Style};
use tui::widgets::{Block, Borders, List, ListItem, ListState};
use tui::Terminal;

struct Gui {
    invalid: bool,
    sel_column: usize,
    columns: [Column; 3],
    show_incoming_links: bool,
    show_only_public: bool,
}

impl Gui {
    fn on_up(&mut self) {
        let list_state = &mut self.columns[self.sel_column].list_state;
        let selected = list_state.selected().unwrap_or(0);
        if selected > 0 {
            self.invalid = true;
            list_state.select(Some(selected - 1));
            for c in self.columns.iter_mut().skip(self.sel_column + 1) {
                c.list_state.select(Some(0));
            }
        }
    }

    fn on_down(&mut self) {
        let list_state = &mut self.columns[self.sel_column].list_state;
        let selected = list_state.selected().unwrap_or(0);
        if selected + 1 < self.columns[self.sel_column].items.len() {
            self.invalid = true;
            list_state.select(Some(selected + 1));
            for c in self.columns.iter_mut().skip(self.sel_column + 1) {
                c.list_state.select(Some(0));
            }
        }
    }
}

struct Column {
    items: Vec<String>,
    list_state: ListState,
}
impl Column {
    fn new(items: Vec<String>) -> Column {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Column { items, list_state }
    }
}

enum Event<I> {
    Input(I),
}

pub fn show_ui(project: &Graph) -> Result<(), failure::Error> {
    let project_names: Vec<&str> = project.components.iter().map(|c| c.nice_name()).collect();
    let mut sorted_projects: Vec<(usize, &str)> = project_names
        .iter()
        .enumerate()
        .map(|(i, s)| (i, *s))
        .collect();
    sorted_projects.sort_by(|a, b| a.1.cmp(b.1));
    let sorted_project_names: Vec<String> =
        sorted_projects.iter().map(|(_i, s)| (*s).into()).collect();

    enable_raw_mode()?;

    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    // Setup input handling
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        loop {
            // poll for tick rate duration, if no events, sent tick event.
            if event::poll(Duration::from_millis(250)).unwrap() {
                if let CEvent::Key(key) = event::read().unwrap() {
                    tx.send(Event::Input(key)).unwrap();
                }
            }
        }
    });

    terminal.clear()?;

    let mut gui = Gui {
        invalid: true,
        sel_column: 0,
        columns: [
            Column::new(sorted_project_names),
            Column::new(vec![]),
            Column::new(vec![]),
        ],
        show_incoming_links: true,
        show_only_public: false,
    };

    loop {
        if gui.invalid {
            let (dep_in, dep_out) = project.linked_components(
                sorted_projects[gui.columns[0].list_state.selected().unwrap_or(0)].0,
                gui.show_only_public,
            );

            let (deps, files) = if gui.show_incoming_links {
                get_dependencies_and_edge_descriptions(&project, dep_in)
            } else {
                get_dependencies_and_edge_descriptions(&project, dep_out)
            };

            gui.columns[1].items = deps;
            gui.columns[2].items = files
                .into_iter()
                .nth(gui.columns[1].list_state.selected().unwrap_or(0))
                .take()
                .unwrap_or_default();
        }

        let mut field_heights = [0, 0, 0];

        terminal.draw(|f| {
            let vertical_split = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(f.size());
            let horizontal_split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(vertical_split[0]);

            let column_rects = [horizontal_split[0], horizontal_split[1], vertical_split[1]];
            field_heights = [
                column_rects[0].height,
                column_rects[1].height,
                column_rects[2].height,
            ];

            let style = Style::default();
            let style_selected = Style::default().fg(Color::White).bg(Color::DarkGray);

            for i in 0..3 {
                let title = match i {
                    0 => "Component (navigate with arrow/page keys)",
                    1 if gui.show_incoming_links => "Incoming (press o for outgoing)",
                    1 => "Outgoing (press i for incoming)",
                    2 if gui.show_only_public => "Files (showing public references, toggle with p)",
                    2 => "Files (showing all references, toggle with p)",
                    _ => unreachable!(),
                };
                let items: Vec<_> = gui.columns[i]
                    .items
                    .iter()
                    .map(|i| ListItem::new(i.as_str()))
                    .collect();
                let list = List::new(items)
                    .block(Block::default().borders(Borders::ALL).title(title))
                    .highlight_symbol(">");
                let list = match gui.sel_column == i {
                    true => list.style(style).highlight_style(style_selected),
                    false => list.style(style).highlight_style(style),
                };
                f.render_stateful_widget(list, column_rects[i], &mut gui.columns[i].list_state);
            }
        })?;

        match rx.recv()? {
            Event::Input(event) => match event.code {
                KeyCode::Char('c') if event.modifiers == KeyModifiers::CONTROL => {
                    disable_raw_mode()?;
                    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                    terminal.show_cursor()?;
                    break;
                }
                KeyCode::Char('i') => {
                    gui.columns[1].list_state.select(Some(0));
                    gui.show_incoming_links = true;
                }
                KeyCode::Char('o') => {
                    gui.columns[1].list_state.select(Some(0));
                    gui.show_incoming_links = false;
                }
                KeyCode::Char('p') => {
                    gui.columns[1].list_state.select(Some(0));
                    gui.show_only_public = !gui.show_only_public;
                }
                KeyCode::Up => {
                    gui.on_up();
                }
                KeyCode::PageUp => {
                    for _ in 0..field_heights[gui.sel_column] {
                        gui.on_up();
                    }
                }
                KeyCode::Down => {
                    gui.on_down();
                }
                KeyCode::PageDown => {
                    for _ in 0..field_heights[gui.sel_column] {
                        gui.on_down();
                    }
                }
                KeyCode::Left => {
                    if gui.sel_column > 0 {
                        gui.sel_column -= 1;
                    }
                }
                KeyCode::Right => {
                    if gui.sel_column < 2 {
                        gui.sel_column += 1;
                    }
                }
                _ => {}
            },
        }
    }

    Ok(())
}

fn get_dependencies_and_edge_descriptions(
    project: &Graph,
    deps: HashMap<ComponentRef, Vec<Edge>>,
) -> (Vec<String>, Vec<Vec<String>>) {
    let mut sorted_keys: Vec<ComponentRef> = deps.keys().map(|k| *k).collect();
    let sort_fn = |a: &ComponentRef, b: &ComponentRef| {
        project.components[*a]
            .path
            .cmp(&project.components[*b].path)
    };
    sorted_keys.sort_by(sort_fn);
    let dep_names = sorted_keys
        .iter()
        .map(|&c_ref| project.components[c_ref].nice_name().into())
        .collect();
    let files = sorted_keys
        .into_iter()
        .map(|c_ref| {
            deps[&c_ref]
                .iter()
                .map(|e| {
                    format!(
                        "{} -> {}",
                        project.files[e.from].path, project.files[e.to].path
                    )
                })
                .collect()
        })
        .collect();
    (dep_names, files)
}
