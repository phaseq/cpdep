use crate::file_collector::{self, Component, File};
use crate::Opt;
use std::collections::HashMap;

pub struct Project {
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

pub fn load(options: &crate::Opt) -> Project {
    let base_project = file_collector::read_files(&options);
    let file_components = files_to_components(&base_project);
    let mut component_files = vec![vec![]; base_project.components.len()];
    for (i, &c) in file_components.iter().enumerate() {
        component_files[c].push(i);
    }
    let file_links = generate_file_links(&base_project.files, &file_components, &options);

    Project {
        files: base_project.files,
        components: base_project.components,
        file_components,
        component_files,
        file_links,
    }
}

impl Project {
    pub fn print_components(
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

    pub fn print_component(
        &self,
        c: ComponentRef,
        component_to: &Option<String>,
        show_files: bool,
    ) {
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
                // If a file can be included from the current solution, assume that it is.
                // This avoids adding dependencies to headers with name clashes (like StdAfx.h).
                let is_present_in_this_component = deps
                    .iter()
                    .any(|f| file_components[*f] == file_components[i_file]);
                if !is_present_in_this_component {
                    for dep in deps.iter() {
                        file_links[i_file].outgoing_links.push(*dep);
                        file_links[*dep].incoming_links.push(i_file);
                    }
                }
            } else if options.warn_missing {
                println!("include not found in {}: {}", file.path, include);
            }
        }
    }
    file_links
}
