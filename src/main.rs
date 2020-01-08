use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::io::{self, stdout, Read, Write};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use structopt::StructOpt;
use tui::backend::CrosstermBackend;
use tui::layout::{Constraint, Direction, Layout};
use tui::style::{Color, Style};
use tui::widgets::{Block, Borders, SelectableList, Widget};
use tui::Terminal;

lazy_static! {
    static ref INCLUDE_RE: regex::bytes::Regex =
        regex::bytes::Regex::new("#\\s*include\\s*[<\"]([^>\"]+)").unwrap();
    static ref INCLUDE_RE_16: regex::bytes::Regex =
        regex::bytes::Regex::new("#\0[\\s\0]*i\0n\0c\0l\0u\0d\0e\0[\\s\0]*[<\"]\0([^>\"]+)")
            .unwrap();
}

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(long)]
    root: String,

    component_from: Option<String>,
    component_to: Option<String>,

    /// warn about missing includes
    #[structopt(long)]
    warn_missing: bool,

    /// warn about malformed includes
    #[structopt(long)]
    warn_malformed: bool,

    /// show files for dependencies
    #[structopt(long)]
    show_files: bool,

    /// show terminal UI
    #[structopt(long)]
    show_ui: bool,
}

fn main() -> Result<(), failure::Error> {
    let options = Opt::from_args();
    let mut project = read_files(&options)?;
    project.assign_files_to_components();
    project.generate_file_deps(&options);

    if options.show_ui {
        show_ui(&project)?;
    } else {
        project.print_components(&options);
    }

    Ok(())
}

type ComponentRef = usize;
type FileRef = usize;

#[derive(Debug)]
struct File {
    path: String,
    include_paths: Vec<String>,

    component: Option<ComponentRef>,
    incoming_links: Vec<FileRef>,
    outgoing_links: Vec<FileRef>,
}

#[derive(Debug)]
struct Component {
    path: String,
    files: Vec<FileRef>,
}

impl Component {
    fn nice_name(&self) -> &str {
        if self.path.is_empty() {
            return ".";
        }
        return &self.path;
    }
}

struct Edge {
    from: FileRef,
    to: FileRef,
}

#[derive(Debug)]
struct Project {
    root: String,
    files: Vec<File>,
    components: Vec<Component>,
}

impl Project {
    fn file(&self, r: FileRef) -> &File {
        return &self.files[r];
    }

    fn component(&self, r: ComponentRef) -> &Component {
        return &self.components[r];
    }

    fn rel_path<'a>(&self, path: &'a str) -> &'a str {
        return path.trim_start_matches(&self.root).trim_start_matches('/');
    }

    fn print_components(&self, options: &Opt) {
        for (c_ref, c) in self.components.iter().enumerate() {
            let c_name = c.nice_name();
            if options
                .component_from
                .as_ref()
                .map(|f| f == c_name)
                .unwrap_or(true)
            {
                self.print_component(c_ref, options);
            }
        }
    }

    fn print_component(&self, c: ComponentRef, options: &Opt) {
        println!(
            "{} ({})",
            self.component(c).nice_name(),
            self.component(c).files.len()
        );

        let (dep_in, dep_out) = self.linked_components(c);

        let print_deps = |deps: HashMap<ComponentRef, Vec<Edge>>| {
            let mut sorted_keys: Vec<ComponentRef> = deps.keys().map(|k| *k).collect();
            let sort_fn = |a: &ComponentRef, b: &ComponentRef| {
                self.component(*a).path.cmp(&self.component(*b).path)
            };
            sorted_keys.sort_by(sort_fn);
            for c_ref in sorted_keys {
                let name = self.component(c_ref).nice_name();
                if options
                    .component_to
                    .as_ref()
                    .map(|t| t == name)
                    .unwrap_or(true)
                {
                    println!("    {}", name);
                    if options.show_files {
                        for e in &deps[&c_ref] {
                            println!(
                                "      {} -> {}",
                                self.file(e.from).path,
                                self.file(e.to).path
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
        for f in self.component(c).files.iter() {
            for fo in self.file(*f).incoming_links.iter() {
                let co = self.file(*fo).component.unwrap();
                if co != c {
                    incoming
                        .entry(co)
                        .or_default()
                        .push(Edge { from: *fo, to: *f })
                }
            }
            for fo in self.file(*f).outgoing_links.iter() {
                let co = self.file(*fo).component;
                if co.is_none() {
                    println!("no component found: {:?}", self.file(*fo));
                }
                let co = co.unwrap();
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

    fn assign_files_to_components(&mut self) {
        for (i_file, file) in self.files.iter_mut().enumerate() {
            // Iterate over prefixes of file path, to find the most specific component.
            // Assign to the most specific component.
            // Example: file path: "a/b/header.hpp"
            // candidate 1: a/b
            // candidate 2: a
            // candidate 3: ''
            let mut path = file.path.clone();
            let mut found = false;
            for (idx, _) in file.path.rmatch_indices('/') {
                path.truncate(idx);
                if let Some((i, c)) = self
                    .components
                    .iter_mut()
                    .enumerate()
                    .find(|(_, c)| c.path == path)
                {
                    file.component = Some(i);
                    c.files.push(i_file);
                    found = true;
                    break;
                }
            }
            if !found {
                if let Some((i, c)) = self
                    .components
                    .iter_mut()
                    .enumerate()
                    .find(|(_, c)| c.path.is_empty())
                {
                    file.component = Some(i);
                    c.files.push(i_file);
                }
            }
        }
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
            let include_paths = self.files[i_file].include_paths.clone(); // TODO: get rid of this?
            for include in include_paths.iter() {
                let deps = path_to_files.get(include.into());
                if let Some(deps) = deps {
                    // If a file can be included from the current solution, assume that it is.
                    // This avoids adding dependencies to headers with name clashes (like StdAfx.h).
                    let is_present_in_this_component = deps
                        .iter()
                        .any(|f| self.file(*f).component == self.files[i_file].component);
                    if !is_present_in_this_component {
                        for dep in deps.iter() {
                            self.files[i_file].outgoing_links.push(*dep);
                            self.files[*dep].incoming_links.push(i_file);
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

fn read_files(options: &Opt) -> io::Result<Project> {
    let source_suffixes = [".cpp", ".hpp", ".c", ".h"];
    //let ignore_patterns = [".svn", "dev/tools"];

    let root_path = options.root.replace('\\', "/");
    let root_path = root_path.trim_end_matches("/");

    let project = std::sync::Arc::new(std::sync::Mutex::new(Project {
        root: root_path.into(),
        files: Vec::new(),
        components: Vec::new(),
    }));

    let warn_malformed = options.warn_malformed;

    ignore::WalkBuilder::new(root_path.to_owned())
        .threads(6)
        .build_parallel()
        .run(|| {
            Box::new({
                let project = project.clone();
                move |result: std::result::Result<ignore::DirEntry, ignore::Error>| {
                    match result {
                        Ok(entry) => {
                            let path_str = entry
                                .path()
                                .to_str()
                                .expect("failed to parse file name")
                                .replace('\\', "/");
                            if entry.path().ends_with("CMakeLists.txt") {
                                let path = path_str.trim_end_matches("/CMakeLists.txt");
                                let mut project = project.lock().unwrap();
                                let path = project.rel_path(path).to_string();
                                project.components.push(Component {
                                    path,
                                    files: vec![],
                                });
                            } else if source_suffixes.iter().any(|s| path_str.ends_with(s)) {
                                match extract_includes(&entry.path(), warn_malformed) {
                                    Ok(include_paths) => {
                                        let mut project = project.lock().unwrap();
                                        let path = project.rel_path(&path_str).to_string();
                                        project.files.push(File {
                                            path,
                                            component: None,
                                            include_paths: include_paths,
                                            incoming_links: vec![],
                                            outgoing_links: vec![],
                                        })
                                    }
                                    Err(e) => println!("Error while parsing {}: {}", path_str, e),
                                }
                            }
                        }
                        Err(e) => panic!("{}", e), // TODO
                    }
                    return ignore::WalkState::Continue;
                }
            })
        });

    let lock = std::sync::Arc::try_unwrap(project).unwrap();
    Ok(lock.into_inner().unwrap())
}

fn extract_includes(path: &Path, warn_malformed: bool) -> io::Result<Vec<String>> {
    let mut results = Vec::new();
    let mut f = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes)?;

    for cap in INCLUDE_RE.captures_iter(&bytes) {
        let mut include = String::from_utf8_lossy(&cap[1]).replace('\\', "/");
        if let Some(idx) = include.rfind("../") {
            if warn_malformed {
                println!("malformed include in {:?}: {}", path, include);
            }
            include = include.split_off(idx + 3);
        }
        results.push(include);
    }

    if results.is_empty() {
        for cap in INCLUDE_RE_16.captures_iter(&bytes) {
            let include_bytes: Vec<u16> = cap[1]
                .chunks_exact(2)
                .into_iter()
                .map(|a| u16::from_ne_bytes([a[0], a[1]]))
                .collect();
            let mut include = String::from_utf16_lossy(&include_bytes).replace('\\', "/");
            if let Some(idx) = include.rfind("../") {
                if warn_malformed {
                    println!("malformed include in {:?}: {}", path, include);
                }
                include = include.split_off(idx + 3);
            }
            results.push(include);
        }
    }

    Ok(results)
}

struct Gui {
    invalid: bool,
    sel_column: usize,
    columns: [Column; 3],
    show_incoming_links: bool,
}

impl Gui {
    fn on_up(&mut self) {
        let selected = &mut self.columns[self.sel_column].selected;
        if *selected > 0 {
            self.invalid = true;
            *selected -= 1;
            for c in self.columns.iter_mut().skip(self.sel_column + 1) {
                c.selected = 0;
            }
        }
    }

    fn on_down(&mut self) {
        let selected = &mut self.columns[self.sel_column].selected;
        if *selected + 1 < self.columns[self.sel_column].items.len() {
            self.invalid = true;
            *selected += 1;
            for c in self.columns.iter_mut().skip(self.sel_column + 1) {
                c.selected = 0;
            }
        }
    }
}

struct Column {
    items: Vec<String>,
    selected: usize,
}
impl Column {
    fn new(items: Vec<String>) -> Column {
        Column { items, selected: 0 }
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
            let (dep_in, dep_out) =
                project.linked_components(sorted_projects[gui.columns[0].selected].0);

            let (deps, files) = match gui.show_incoming_links {
                true => get_dependencies_and_edge_descriptions(&project, dep_in),
                false => get_dependencies_and_edge_descriptions(&project, dep_out),
            };

            gui.columns[1].items = deps;
            gui.columns[2].items = files
                .into_iter()
                .nth(gui.columns[1].selected)
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
                let list = SelectableList::default()
                    .block(Block::default().borders(Borders::ALL).title(title))
                    .highlight_symbol(">")
                    .items(&gui.columns[i].items)
                    .select(Some(gui.columns[i].selected));
                let mut list = match gui.sel_column == i {
                    true => list.style(style).highlight_style(style_selected),
                    false => list.style(style).highlight_style(style),
                };
                list.render(&mut f, column_rects[i]);
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
                    gui.columns[1].selected = 0;
                    gui.show_incoming_links = true;
                }
                KeyCode::Char('o') => {
                    gui.columns[1].selected = 0;
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
        project.component(*a).path.cmp(&project.component(*b).path)
    };
    sorted_keys.sort_by(sort_fn);
    let dep_names = sorted_keys
        .iter()
        .map(|&c_ref| project.component(c_ref).nice_name().into())
        .collect();
    let files = sorted_keys
        .into_iter()
        .map(|c_ref| {
            deps[&c_ref]
                .iter()
                .map(|e| {
                    format!(
                        "{} -> {}",
                        project.file(e.from).path,
                        project.file(e.to).path
                    )
                })
                .collect()
        })
        .collect();
    (dep_names, files)
}
