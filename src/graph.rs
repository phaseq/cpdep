use crate::file_collector::{self, Component, File};
use crate::Opt;
use std::collections::HashMap;

pub struct Graph {
    pub files: Vec<File>,
    pub components: Vec<Component>,
    pub file_components: Vec<ComponentRef>,
    pub component_files: Vec<Vec<FileRef>>,
    pub file_links: Vec<FileLinks>,
    pub file_is_public: Vec<bool>,
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
    let file_is_public = generate_is_public(&file_links, &file_components);

    Graph {
        files: base_project.files,
        components: base_project.components,
        file_components,
        component_files,
        file_links,
        file_is_public,
    }
}

impl Graph {
    /*pub fn shortest_path_to_public(&self, f_from: FileRef) -> Option<Vec<FileRef>> {
        let c_from = self.file_components[f_from];

        let mut dists = vec![(0usize, u32::max_value()); self.files.len()];
        dists[f_from] = (f_from, 0);

        let mut queue = std::collections::VecDeque::new();
        queue.push_back(f_from);

        while let Some(f_source) = queue.pop_front() {
            let dist = dists[f_source].1 + 1;

            for &fi in self.file_links[f_source].incoming_links.iter() {
                let c_to = self.file_components[fi];
                if c_to != c_from {
                    // found
                    let mut result = vec![];
                    let mut f = fi;
                    while f != f_from {
                        result.push(f);
                        f = dists[f].0;
                    }
                    result.push(c_from);
                    result.reverse();
                    return Some(result);
                }
                if dists[fi].1 > dist {
                    dists[fi] = (f_source, dist);
                    queue.push_back(fi);
                }
            }
        }
        None
    }*/

    pub fn is_header(&self, file_ref: FileRef) -> bool {
        let path = &self.files[file_ref].path;
        path.ends_with(".h") || path.ends_with(".hpp") || path.ends_with("hxx")
    }

    /*fn is_source_file(&self, file_ref: FileRef) -> bool {
        let path = &self.files[file_ref].path;
        path.ends_with(".cpp") || path.ends_with(".c")
    }*/

    pub fn component_name_to_ref(&self, component_from: &str) -> Option<ComponentRef> {
        self.components
            .iter()
            .enumerate()
            .find(|(_i, c)| c.nice_name() == component_from)
            .map(|(i, _)| i)
    }

    pub fn linked_components(
        &self,
        c: ComponentRef,
        only_public: bool,
    ) -> (
        HashMap<ComponentRef, Vec<Edge>>,
        HashMap<ComponentRef, Vec<Edge>>,
    ) {
        let mut incoming: HashMap<ComponentRef, Vec<Edge>> = HashMap::new();
        let mut outgoing: HashMap<ComponentRef, Vec<Edge>> = HashMap::new();
        for &f in self.component_files[c].iter() {
            for &fi in self.file_links[f].incoming_links.iter() {
                if !only_public || self.file_is_public[fi] {
                    let co = self.file_components[fi];
                    if co != c {
                        incoming
                            .entry(co)
                            .or_default()
                            .push(Edge { from: fi, to: f })
                    }
                }
            }
            if !only_public || self.file_is_public[f] {
                for &fo in self.file_links[f].outgoing_links.iter() {
                    let co = self.file_components[fo];
                    if co != c {
                        outgoing
                            .entry(co)
                            .or_default()
                            .push(Edge { from: f, to: fo })
                    }
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

            default_component
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

fn generate_is_public(file_links: &[FileLinks], file_components: &[ComponentRef]) -> Vec<bool> {
    let mut is_public = vec![false; file_links.len()];
    let mut to_visit: std::collections::VecDeque<FileRef> = std::collections::VecDeque::new();

    for (f, links) in file_links.iter().enumerate() {
        for &fi in &links.outgoing_links {
            if !is_public[fi] && file_components[fi] != file_components[f] {
                is_public[fi] = true;
                to_visit.push_back(fi);
            }
        }
    }

    while let Some(f) = to_visit.pop_front() {
        for &fi in &file_links[f].outgoing_links {
            if !is_public[fi] && file_components[fi] != file_components[f] {
                is_public[fi] = true;
                to_visit.push_back(fi);
            }
        }
    }

    is_public
}
