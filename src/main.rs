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
use structopt::StructOpt;
use tui::backend::CrosstermBackend;
use tui::layout::{Constraint, Direction, Layout};
use tui::style::{Color, Style};
use tui::widgets::{Block, Borders, List, ListState, Text};
use tui::Terminal;

mod file_collector;
use file_collector::{Component, File};

#[derive(StructOpt)]
pub struct Opt {
    #[structopt(long)]
    root: String,

    /// warn about missing includes
    #[structopt(long)]
    warn_missing: bool,

    /// warn about malformed includes
    #[structopt(long)]
    warn_malformed: bool,

    #[structopt(subcommand)]
    cmd: Cmd,
}

#[derive(StructOpt)]
enum Cmd {
    // show direct links between components
    Links {
        /// show incoming and outgoing links for this component
        component_from: Option<String>,

        // giving a second component restricts links further
        component_to: Option<String>,

        /// show files for dependencies
        #[structopt(long)]
        show_files: bool,
    },
    /// show terminal UI
    UI {},
    /// show all strongly connected components
    Scc {},
}

fn main() -> Result<(), failure::Error> {
    let options = Opt::from_args();
    let base_project = file_collector::read_files(&options);
    let file_components = files_to_components(&base_project);
    let mut component_files = vec![vec![]; base_project.components.len()];
    for (i, &c) in file_components.iter().enumerate() {
        component_files[c].push(i);
    }

    let n_files = base_project.files.len();
    let mut project = Project {
        files: base_project.files,
        components: base_project.components,
        file_components,
        component_files,
        file_links: vec![FileLinks::default(); n_files],
    };
    project.generate_file_deps(&options);

    match options.cmd {
        Cmd::Links {
            component_from,
            component_to,
            show_files,
        } => project.print_components(component_from, component_to, show_files),
        Cmd::UI {} => show_ui(&project)?,
        Cmd::Scc {} => show_sccs(&project),
    }

    Ok(())
}

struct Project {
    files: Vec<File>,
    components: Vec<Component>,
    file_components: Vec<ComponentRef>,
    component_files: Vec<Vec<FileRef>>,
    file_links: Vec<FileLinks>,
}

#[derive(Clone, Default)]
struct FileLinks {
    incoming_links: Vec<FileRef>,
    outgoing_links: Vec<FileRef>,
}

type ComponentRef = usize;
type FileRef = usize;

struct Edge {
    from: FileRef,
    to: FileRef,
}

fn files_to_components(base_project: &file_collector::FileCollector) -> Vec<ComponentRef> {
    let default_component = base_project
        .components
        .iter()
        .enumerate()
        .find(|(_, c)| c.path.is_empty())
        .map(|(i, _)| i)
        .unwrap();

    base_project
        .files
        .iter()
        .map(|file| {
            // Iterate over prefixes of file path, to find the most specific component.
            // Assign to the most specific component.
            // Example: file path: "a/b/header.hpp"
            // candidate 1: a/b
            // candidate 2: a
            // candidate 3: ''
            let mut path = file.path.clone();
            for (idx, _) in file.path.rmatch_indices('/') {
                path.truncate(idx);
                if let Some((i, _c)) = base_project
                    .components
                    .iter()
                    .enumerate()
                    .find(|(_, c)| c.path == path)
                {
                    return i;
                }
            }

            return default_component;
        })
        .collect()
}

impl Project {
    fn print_components(
        &self,
        component_from: Option<String>,
        component_to: Option<String>,
        show_files: bool,
    ) {
        for (c_ref, c) in self.components.iter().enumerate() {
            let c_name = c.nice_name();
            if component_from.as_ref().map(|f| f == c_name).unwrap_or(true) {
                self.print_component(c_ref, &component_to, show_files);
            }
        }
    }

    fn print_component(&self, c: ComponentRef, component_to: &Option<String>, show_files: bool) {
        println!(
            "{} ({})",
            self.components[c].nice_name(),
            self.component_files[c].len()
        );

        let (dep_in, dep_out) = self.linked_components(c);

        let print_deps = |deps: HashMap<ComponentRef, Vec<Edge>>| {
            let mut sorted_keys: Vec<ComponentRef> = deps.keys().map(|k| *k).collect();
            let sort_fn = |a: &ComponentRef, b: &ComponentRef| {
                self.components[*a].path.cmp(&self.components[*b].path)
            };
            sorted_keys.sort_by(sort_fn);
            for c_ref in sorted_keys {
                let name = self.components[c_ref].nice_name();
                if component_to.as_ref().map(|t| t == name).unwrap_or(true) {
                    println!("    {}", name);
                    if show_files {
                        for e in &deps[&c_ref] {
                            println!(
                                "      {} -> {}",
                                self.files[e.from].path, self.files[e.to].path
                            );
                        }
                    }
                }
            }
        };

        println!("  Incoming:");
        print_deps(dep_in);

        println!("  Outgoing:");
        print_deps(dep_out);
    }

    fn linked_components(
        &self,
        c: ComponentRef,
    ) -> (
        HashMap<ComponentRef, Vec<Edge>>,
        HashMap<ComponentRef, Vec<Edge>>,
    ) {
        let mut incoming: HashMap<ComponentRef, Vec<Edge>> = HashMap::new();
        let mut outgoing: HashMap<ComponentRef, Vec<Edge>> = HashMap::new();
        for f in self.component_files[c].iter() {
            for fo in self.file_links[*f].incoming_links.iter() {
                let co = self.file_components[*fo];
                if co != c {
                    incoming
                        .entry(co)
                        .or_default()
                        .push(Edge { from: *fo, to: *f })
                }
            }
            for fo in self.file_links[*f].outgoing_links.iter() {
                let co = self.file_components[*fo];
                if co != c {
                    outgoing
                        .entry(co)
                        .or_default()
                        .push(Edge { from: *f, to: *fo })
                }
            }
        }

        (incoming, outgoing)
    }

    fn generate_file_deps(&mut self, options: &Opt) {
        // map from possible include paths to corresponding files
        // for example: "a/b/header.h" could be included as "header.h", "b/header.h", and "a/b/header.h"
        // assumption here: normalized paths with unix slashes
        let mut path_to_files: HashMap<String, Vec<FileRef>> = HashMap::new();
        for (i_file, file) in self.files.iter().enumerate() {
            path_to_files
                .entry(file.path.clone())
                .or_default()
                .push(i_file);
            for (idx, _) in file.path.match_indices('/') {
                path_to_files
                    .entry(file.path[idx + 1..].into())
                    .or_default()
                    .push(i_file);
            }
        }

        for i_file in 0..self.files.len() {
            for include in self.files[i_file].include_paths.iter() {
                let deps = path_to_files.get(include);
                if let Some(deps) = deps {
                    // If a file can be included from the current solution, assume that it is.
                    // This avoids adding dependencies to headers with name clashes (like StdAfx.h).
                    let is_present_in_this_component = deps
                        .iter()
                        .any(|f| self.file_components[*f] == self.file_components[i_file]);
                    if !is_present_in_this_component {
                        for dep in deps.iter() {
                            self.file_links[i_file].outgoing_links.push(*dep);
                            self.file_links[*dep].incoming_links.push(i_file);
                        }
                    }
                } else if options.warn_missing {
                    println!(
                        "include not found in {}: {}",
                        self.files[i_file].path, include
                    );
                }
            }
        }
    }
}

fn show_sccs(project: &Project) {
    let sccs = Tarjan::run(project);

    for mut scc in sccs.into_iter().filter(|c| c.len() > 1) {
        scc.reverse();
        println!("Strongly Connected:");
        for c_ref in scc {
            println!("  {}", project.components[c_ref].nice_name());
        }
    }
}

struct Tarjan {
    index: i32,
    indices: Vec<i32>,
    lowlink: Vec<i32>,
    on_stack: Vec<bool>,
    stack: Vec<ComponentRef>,
    sccs: Vec<Vec<ComponentRef>>,
}

impl Tarjan {
    fn run(project: &Project) -> Vec<Vec<ComponentRef>> {
        let mut t = Tarjan {
            index: 0,
            indices: std::iter::repeat(-1)
                .take(project.components.len())
                .collect(),
            lowlink: std::iter::repeat(-1)
                .take(project.components.len())
                .collect(),
            on_stack: std::iter::repeat(false)
                .take(project.components.len())
                .collect(),
            stack: vec![],
            sccs: vec![],
        };
        for v in 0..project.components.len() {
            if t.indices[v] == -1 {
                t.strong_connect(v, project);
            }
        }
        t.sccs
    }

    fn strong_connect(&mut self, v: ComponentRef, project: &Project) {
        // Set the depth index for v to the smallest unused index
        self.indices[v] = self.index;
        self.lowlink[v] = self.index;
        self.index += 1;
        self.stack.push(v);
        self.on_stack[v] = true;

        // Consider successors of v
        for w in project.component_files[v]
            .iter()
            .flat_map(|&f| &project.file_links[f].outgoing_links)
            .map(|f| project.file_components[*f])
            .filter(|&c| c != v)
        {
            if self.indices[w] == -1 {
                // Successor w has not yet been visited; recurse on it
                self.strong_connect(w, &project);
                self.lowlink[v] = std::cmp::min(self.lowlink[v], self.lowlink[w]);
            } else if self.on_stack[w] {
                // Successor w is in stack S and hence in the current SCC
                // If w is not on stack, then (v, w) is a cross-edge in the DFS tree and must be ignored
                // Note: The next line may look odd - but is correct.
                // It says w.index not w.lowlink; that is deliberate and from the original paper
                self.lowlink[v] = std::cmp::min(self.lowlink[v], self.indices[w]);
            }
        }
        // If v is a root node, pop the stack and generate an SCC
        if self.lowlink[v] == self.indices[v] {
            let mut scc = vec![];
            loop {
                let w = self.stack.pop().expect("empty stack?");
                self.on_stack[w] = false;
                scc.push(w);
                if w == v {
                    break;
                }
            }
            self.sccs.push(scc);
        }
    }
}

struct Gui {
    invalid: bool,
    sel_column: usize,
    columns: [Column; 3],
    show_incoming_links: bool,
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

fn show_ui(project: &Project) -> Result<(), failure::Error> {
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
    };

    loop {
        if gui.invalid {
            let (dep_in, dep_out) = project.linked_components(
                sorted_projects[gui.columns[0].list_state.selected().unwrap_or(0)].0,
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

        terminal.draw(|mut f| {
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
                    2 => "Files",
                    _ => unreachable!(),
                };
                let items = gui.columns[i].items.iter().map(|i| Text::raw(i.clone()));
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
    project: &Project,
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
