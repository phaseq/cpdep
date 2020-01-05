use std::collections::HashMap;
use std::io::{self, Read};
use std::path::Path;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(long)]
    root: String,

    components: Vec<String>,

    /// warn about missing includes
    #[structopt(long)]
    warn_missing: bool,

    /// show files for incoming dependencies
    #[structopt(long)]
    show_incoming: bool,

    /// show files for outgoing dependencies
    #[structopt(long)]
    show_outgoing: bool,
}

fn main() -> io::Result<()> {
    let opt = Opt::from_args();
    let mut project = read_files(&opt.root)?;
    project.assign_files_to_components();
    project.generate_file_deps(&opt);
    project.print_components(&opt);
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
            if options.components.is_empty() || options.components.iter().any(|x| x == c_name) {
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
        println!("  Incoming:");
        for (c_ref, edges) in &dep_in {
            println!("    {}", self.component(*c_ref).nice_name());
            if options.show_incoming {
                for e in edges {
                    println!(
                        "      {} -> {}",
                        self.file(e.from).path,
                        self.file(e.to).path
                    );
                }
            }
        }

        println!("  Outgoing:");
        for (c_ref, edges) in &dep_out {
            println!("    {}", self.component(*c_ref).nice_name());
            if options.show_outgoing {
                for e in edges {
                    println!(
                        "      {} -> {}",
                        self.file(e.from).path,
                        self.file(e.to).path
                    );
                }
            }
        }
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
            let include_paths = self.files[i_file].include_paths.clone(); // TODO: get rid of this clone?
            for include in include_paths.iter() {
                let deps = path_to_files.get(include.into());
                if let Some(deps) = deps {
                    // If a file can be included from the current solution, assume that it is.
                    // This avoids adding dependencies to headers with name clashes (like StdAfx.h).
                    let is_present_in_this_component = false; /*deps
                                                              .iter()
                                                              .any(|f| self.file(*f).component == file.component);*/
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

fn read_files(root_path: &str) -> io::Result<Project> {
    let source_suffixes = [".cpp", ".hpp", ".c", ".h"];
    //let ignore_patterns = [".svn", "dev/tools"];

    let root_path = root_path.trim_end_matches("/");

    let mut project = Project {
        root: root_path.into(),
        files: Vec::new(),
        components: Vec::new(),
    };

    for result in ignore::WalkBuilder::new(root_path.to_owned()).build() {
        match result {
            Ok(entry) => {
                let path_str = entry.path().to_str().expect("failed to parse file name");
                if entry.path().ends_with("CMakeLists.txt") {
                    let path = path_str.trim_end_matches("/CMakeLists.txt");
                    project.components.push(Component {
                        path: project.rel_path(path).to_string(),
                        files: vec![],
                    });
                } else if source_suffixes.iter().any(|s| path_str.ends_with(s)) {
                    match extract_includes(&entry.path()) {
                        Ok(include_paths) => project.files.push(File {
                            path: project.rel_path(path_str).to_string(),
                            component: None,
                            include_paths: include_paths,
                            incoming_links: vec![],
                            outgoing_links: vec![],
                        }),
                        Err(e) => println!("Error while parsing {}: {}", path_str, e),
                    }
                }
            }
            Err(e) => panic!("{}", e), // TODO
        }
    }

    Ok(project)
}

fn extract_includes(path: &Path) -> io::Result<Vec<String>> {
    let mut results = Vec::new();
    let mut f = std::fs::File::open(path)?;
    let mut c = Vec::new();
    f.read_to_end(&mut c)?;
    let re = regex::bytes::Regex::new("#include [<\"]([^>\"]+)[>\"]").unwrap();
    for cap in re.captures_iter(&c) {
        results.push(String::from_utf8_lossy(&cap[1]).into());
    }
    Ok(results)
}
