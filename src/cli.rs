use crate::graph::{ComponentRef, Edge, FileRef, Graph};
use std::collections::HashMap;

pub fn print_components(
    graph: &Graph,
    component_from: Option<String>,
    component_to: Option<String>,
    verbose: bool,
    only_public: bool,
) {
    for (c_ref, c) in graph.components.iter().enumerate() {
        let c_name = c.nice_name();
        if component_from.as_ref().map(|f| f == c_name).unwrap_or(true) {
            print_component(&graph, c_ref, &component_to, verbose, only_public);
        }
    }
}

fn print_component(
    graph: &Graph,
    c: ComponentRef,
    component_to: &Option<String>,
    verbose: bool,
    only_public: bool,
) {
    println!(
        "{} ({})",
        graph.components[c].nice_name(),
        graph.component_files[c].len()
    );

    let (dep_in, dep_out) = graph.linked_components(c, only_public);

    let print_deps = |deps: HashMap<ComponentRef, Vec<Edge>>| {
        let mut sorted_keys: Vec<ComponentRef> = deps.keys().cloned().collect();
        let sort_fn = |a: &ComponentRef, b: &ComponentRef| {
            graph.components[*a].path.cmp(&graph.components[*b].path)
        };
        sorted_keys.sort_by(sort_fn);
        for c_ref in sorted_keys {
            let name = graph.components[c_ref].nice_name();
            if component_to.as_ref().map(|t| t == name).unwrap_or(true) {
                println!("    {}", name);
                if verbose {
                    for e in &deps[&c_ref] {
                        println!(
                            "      {} -> {}",
                            graph.files[e.from].path, graph.files[e.to].path
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

pub fn print_file_info(graph: &Graph, file_name: &str) {
    let f_ref = get_file_ref_or_fail(&graph, &file_name);

    println!("Incoming:");
    for &fi in &graph.file_links[f_ref].incoming_links {
        println!("  {}", graph.files[fi].path);
    }

    println!("Outgoing:");
    for &fo in &graph.file_links[f_ref].outgoing_links {
        println!("  {}", graph.files[fo].path);
    }
}

pub fn print_headers(graph: &Graph, component_name: String, verbose: bool) {
    let c_ref = get_component_ref_or_fail(&graph, &component_name);

    let mut public_headers = vec![];
    let mut private_headers = vec![];
    let mut solo_headers = vec![];
    let mut dead_headers = vec![];
    for &file_ref in &graph.component_files[c_ref] {
        let links = &graph.file_links[file_ref].incoming_links;
        let c = graph.file_components[file_ref];
        let public_links: Vec<FileRef> = links
            .iter()
            .filter(|&f_ref| graph.file_components[*f_ref] != c || graph.file_is_public[*f_ref])
            .cloned()
            .collect();
        if !public_links.is_empty() {
            public_headers.push((file_ref, public_links));
            continue;
        }

        if graph.is_header(file_ref) {
            if !links.is_empty() {
                if links.len() == 1 {
                    let fi = links[0];
                    let base_name = graph.files[file_ref].path.rsplit('/').next().unwrap();
                    if let Some(base_name) = base_name.rsplit('.').nth(1) {
                        if graph.files[fi].path.contains(base_name) {
                            // solo header: file included only once, by a similarly-named source file
                            solo_headers.push((file_ref, vec![fi]));
                            continue;
                        }
                    }
                }
                private_headers.push((file_ref, vec![]));
            } else {
                dead_headers.push((file_ref, vec![]));
            }
        }
    }

    let mut sections = [
        ("Public", public_headers),
        ("Private", private_headers),
        ("Solo", solo_headers),
        ("Dead", dead_headers),
    ];
    for (title, headers) in sections.iter_mut() {
        if headers.is_empty() {
            continue;
        }
        println!("{} headers:", title);
        headers.sort_by(|&(f1, _), &(f2, _)| graph.files[f1].path.cmp(&graph.files[f2].path));
        for (f, incoming_links) in headers {
            println!("  {}", graph.files[*f].path);
            if verbose {
                for &fi in &*incoming_links {
                    println!("    <- {}", graph.files[fi].path);
                }
            }
        }
    }
}

pub fn print_shortest(
    graph: &Graph,
    component_from: &str,
    component_to: &str,
    verbose: bool,
    only_public: bool,
) {
    let c_from = get_component_ref_or_fail(&graph, component_from);
    let c_to = get_component_ref_or_fail(&graph, component_to);

    let mut dists = vec![(0usize, u32::max_value()); graph.components.len()];
    dists[c_from] = (c_from, 0);

    let mut queue = std::collections::VecDeque::new();
    queue.push_back(c_from);

    while let Some(c_source) = queue.pop_front() {
        let dist = dists[c_source].1 + 1;

        for &f in graph.component_files[c_source].iter() {
            if c_source == c_from && only_public && !graph.file_is_public[f] {
                continue;
            }
            for fo in graph.file_links[f].outgoing_links.iter() {
                let c = graph.file_components[*fo];
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
        println!("{}", graph.components[c].nice_name());
        if verbose && i + 1 != result.len() {
            let c2 = result[i + 1];
            for &f in graph.component_files[c].iter() {
                if c == c_from && only_public && !graph.file_is_public[f] {
                    continue;
                }
                for &fo in graph.file_links[f].outgoing_links.iter() {
                    if graph.file_components[fo] == c2 {
                        println!("  {} -> {}", graph.files[f].path, graph.files[fo].path);
                    }
                }
            }
        }
    }
}

pub fn show_sccs(project: &Graph) {
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
    fn run(project: &Graph) -> Vec<Vec<ComponentRef>> {
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

    fn strong_connect(&mut self, v: ComponentRef, project: &Graph) {
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

fn get_component_ref_or_fail(graph: &Graph, component_name: &str) -> ComponentRef {
    match graph.component_name_to_ref(component_name) {
        Some(c) => c,
        None => {
            eprintln!("component not found: {}", component_name);
            std::process::exit(1);
        }
    }
}

fn get_file_ref_or_fail(graph: &Graph, file_name: &str) -> FileRef {
    match graph
        .files
        .iter()
        .enumerate()
        .find(|(_, f)| f.path == file_name)
    {
        Some(f) => f.0,
        None => {
            eprintln!("file not found: {}", file_name);
            std::process::exit(1);
        }
    }
}
