use crate::file_collector::{self, Component, File};
use crate::Opt;
use std::collections::HashMap;

pub struct Graph {
    pub files: Vec<File>,
    pub components: Vec<Component>,
    pub file_components: Vec<ComponentRef>,
    pub component_files: Vec<Vec<FileRef>>,
    pub file_links: Vec<FileLinks>,
}

#[derive(Clone, Default)]
pub struct FileLinks {
    pub incoming_links: Vec<FileRef>,
    pub outgoing_links: Vec<FileRef>,
}

pub type ComponentRef = usize;
pub type FileRef = usize;

pub struct Edge {
    pub from: FileRef,
    pub to: FileRef,
}

pub fn load(options: &crate::Opt) -> Graph {
    let base_project = file_collector::read_files(&options);
    let file_components = files_to_components(&base_project);
    let mut component_files = vec![vec![]; base_project.components.len()];
    for (i, &c) in file_components.iter().enumerate() {
        component_files[c].push(i);
    }
    let file_links = generate_file_links(&base_project.files, &file_components, &options);

    Graph {
        files: base_project.files,
        components: base_project.components,
        file_components,
        component_files,
        file_links,
    }
}

impl Graph {
    pub fn print_components(
        &self,
        component_from: Option<String>,
        component_to: Option<String>,
        verbose: bool,
    ) {
        for (c_ref, c) in self.components.iter().enumerate() {
            let c_name = c.nice_name();
            if component_from.as_ref().map(|f| f == c_name).unwrap_or(true) {
                self.print_component(c_ref, &component_to, verbose);
            }
        }
    }

    pub fn print_component(&self, c: ComponentRef, component_to: &Option<String>, verbose: bool) {
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
                    if verbose {
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

    pub fn print_headers(&self, component_name: String, verbose: bool) {
        let (c_ref, _c) = match self
            .components
            .iter()
            .enumerate()
            .find(|(_, c)| c.nice_name() == component_name)
        {
            Some(c) => c,
            None => {
                eprintln!("component not found: {}", component_name);
                std::process::exit(1);
            }
        };

        let mut public_headers = vec![];
        let mut transitive_headers = vec![];
        let mut private_headers = vec![];
        let mut dead_headers = vec![];
        for &file_ref in &self.component_files[c_ref] {
            let links = &self.file_links[file_ref].incoming_links;
            let c = self.file_components[file_ref];
            let public_links: Vec<FileRef> = links
                .into_iter()
                .filter(|&f_ref| self.file_components[*f_ref] != c)
                .cloned()
                .collect();
            let maybe_public_links: Vec<FileRef> = links
                .into_iter()
                .filter(|&f_ref| self.file_components[*f_ref] == c && self.is_header(*f_ref))
                .cloned()
                .collect();

            if !public_links.is_empty() {
                public_headers.push((file_ref, public_links));
            } else if !maybe_public_links.is_empty() {
                transitive_headers.push((file_ref, maybe_public_links));
            } else if self.is_header(file_ref) {
                if !links.is_empty() {
                    private_headers.push((file_ref, vec![]));
                } else {
                    dead_headers.push((file_ref, vec![]));
                }
            }
        }

        let mut sections = [
            ("Public", public_headers),
            ("Transitive", transitive_headers),
            ("Private", private_headers),
            ("Dead", dead_headers),
        ];
        for (title, headers) in sections.iter_mut() {
            if headers.is_empty() {
                continue;
            }
            println!("{} headers:", title);
            headers.sort_by(|&(f1, _), &(f2, _)| self.files[f1].path.cmp(&self.files[f2].path));
            for (f, incoming_links) in headers {
                println!("  {}", self.files[*f].path);
                if verbose {
                    for &fi in &*incoming_links {
                        println!("    <- {}", self.files[fi].path);
                    }
                }
            }
        }
    }

    pub fn print_file_info(&self, file_name: &str) {
        let file = match self
            .files
            .iter()
            .enumerate()
            .find(|(_, f)| f.path == file_name)
        {
            Some(f) => f,
            None => {
                eprintln!("file not found: {}", file_name);
                std::process::exit(1);
            }
        };
        let (f_ref, _f) = file;

        println!("Incoming:");
        for &fi in &self.file_links[f_ref].incoming_links {
            println!("  {}", self.files[fi].path);
        }

        println!("Outgoing:");
        for &fo in &self.file_links[f_ref].outgoing_links {
            println!("  {}", self.files[fo].path);
        }
    }

    pub fn print_shortest(
        &self,
        component_from: &str,
        component_to: &str,
        verbose: bool,
        only_public: bool,
    ) {
        let c_from = match self.component_name_to_ref(component_from) {
            Some(c) => c,
            None => {
                eprintln!("component not found: {}", component_from);
                std::process::exit(1);
            }
        };
        let c_to = match self.component_name_to_ref(component_to) {
            Some(c) => c,
            None => {
                eprintln!("component not found: {}", component_from);
                std::process::exit(1);
            }
        };

        let mut dists = vec![(0usize, u32::max_value()); self.components.len()];
        dists[c_from] = (c_from, 0);

        let mut queue = std::collections::VecDeque::new();
        queue.push_back(c_from);

        while let Some(c_source) = queue.pop_front() {
            let dist = dists[c_source].1 + 1;

            for f in self.component_files[c_source].iter() {
                if c_source == c_from && only_public && !self.has_incoming_links(*f) {
                    continue;
                }
                for fo in self.file_links[*f].outgoing_links.iter() {
                    let c = self.file_components[*fo];
                    if dists[c].1 > dist {
                        dists[c] = (c_source, dist);
                        queue.push_back(c);
                    }
                }
            }
        }

        if dists[c_to].1 == u32::max_value() {
            println!("No path found.");
            return;
        }

        let mut result = vec![];
        let mut c = c_to;
        while c != c_from {
            result.push(c);
            c = dists[c].0;
        }
        result.push(c_from);
        result.reverse();

        for i in 0..result.len() {
            let c = result[i];
            println!("{}", self.components[c].nice_name());
            if verbose && i + 1 != result.len() {
                let c2 = result[i + 1];
                for f in self.component_files[c].iter() {
                    if c == c_from && only_public && !self.has_incoming_links(*f) {
                        continue;
                    }
                    for fo in self.file_links[*f].outgoing_links.iter() {
                        if self.file_components[*fo] == c2 {
                            println!("  {} -> {}", self.files[*f].path, self.files[*fo].path);
                        }
                    }
                }
            }
        }
    }

    fn has_incoming_links(&self, file_ref: FileRef) -> bool {
        let c = self.file_components[file_ref];
        for &fi in &self.file_links[file_ref].incoming_links {
            let ci = self.file_components[fi];
            if ci != c || self.is_header(fi) {
                return true;
            }
        }
        false
    }

    fn is_header(&self, file_ref: FileRef) -> bool {
        let path = &self.files[file_ref].path;
        path.ends_with(".h") || path.ends_with(".hpp") || path.ends_with("hxx")
    }

    fn is_source_file(&self, file_ref: FileRef) -> bool {
        let path = &self.files[file_ref].path;
        path.ends_with(".cpp") || path.ends_with(".c")
    }

    fn component_name_to_ref(&self, component_from: &str) -> Option<ComponentRef> {
        self.components
            .iter()
            .enumerate()
            .find(|(_i, c)| c.nice_name() == component_from)
            .map(|(i, _)| i)
    }

    pub fn linked_components(
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

fn generate_file_links(
    files: &[File],
    file_components: &[ComponentRef],
    options: &Opt,
) -> Vec<FileLinks> {
    // map from possible include paths to corresponding files
    // for example: "a/b/header.h" could be included as "header.h", "b/header.h", and "a/b/header.h"
    // assumption here: normalized paths with unix slashes
    let mut path_to_files: HashMap<String, Vec<FileRef>> = HashMap::new();
    for (i_file, file) in files.iter().enumerate() {
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

    let mut file_links = vec![FileLinks::default(); files.len()];

    for (i_file, file) in files.iter().enumerate() {
        for include in file.include_paths.iter() {
            let deps = path_to_files.get(include);
            if let Some(deps) = deps {
                let is_present_in_this_component = deps
                    .iter()
                    .any(|f| file_components[*f] == file_components[i_file]);
                for &dep in deps.iter() {
                    if is_present_in_this_component
                        && file_components[dep] != file_components[i_file]
                    {
                        // If a file can be included from the current solution, assume that it is.
                        // This avoids adding dependencies to headers with name clashes (like StdAfx.h).
                        continue;
                    }
                    file_links[i_file].outgoing_links.push(dep);
                    file_links[dep].incoming_links.push(i_file);
                }
            } else if options.warn_missing {
                println!("include not found in {}: {}", file.path, include);
            }
        }
    }
    file_links
}
