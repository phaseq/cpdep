use crate::graph::{ComponentRef, Edge, Graph};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn export(graph: &Graph, path: &str) -> std::io::Result<()> {
    let path = PathBuf::from(path);
    std::fs::create_dir_all(&path).unwrap();

    {
        let html = Index { graph }.to_string();
        let mut f = std::fs::File::create(&path.join("index.html")).unwrap();
        f.write_all(html.as_bytes())?;
    }

    for c_ref in 0..graph.components.len() {
        export_component(&graph, c_ref, &path)?;
    }
    Ok(())
}

fn export_component(graph: &Graph, c: ComponentRef, root: &Path) -> std::io::Result<()> {
    let name = graph.components[c].nice_name();
    let path = root.join(&format!("{}.html", name.replace("/", "__")));

    let (dep_in, dep_out) = graph.linked_components(c, false);

    let html = Page {
        graph: &graph,
        name: &name,
        dep_in: &dep_in,
        dep_out: &dep_out,
    }
    .to_string();

    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(html.as_bytes())?;

    Ok(())
}

fn sorted_dep_keys(graph: &Graph, deps: &HashMap<ComponentRef, Vec<Edge>>) -> Vec<ComponentRef> {
    let mut sorted_keys: Vec<ComponentRef> = deps.keys().cloned().collect();
    let sort_fn = |a: &ComponentRef, b: &ComponentRef| {
        graph.components[*a].path.cmp(&graph.components[*b].path)
    };
    sorted_keys.sort_by(sort_fn);
    sorted_keys
}

fn sorted_components(graph: &Graph) -> Vec<ComponentRef> {
    let mut sorted_keys: Vec<ComponentRef> = (0..graph.components.len()).collect();
    let sort_fn = |a: &ComponentRef, b: &ComponentRef| {
        graph.components[*a].path.cmp(&graph.components[*b].path)
    };
    sorted_keys.sort_by(sort_fn);
    sorted_keys
}

markup::define! {
    Index<'a>(graph: &'a Graph) {
        {markup::doctype()}
        html {
            {Head {title: "ModuleWorks C++ Dependencies"}}
            body {
                h1 { "ModuleWorks C++ Dependencies" }
                ul {
                    @for c in sorted_components(&graph).into_iter().map(|c_ref| &graph.components[c_ref]) {
                        li {
                            {c.nice_name()}
                            " "
                            a[href=format!("{}.html", c.nice_name().replace('/', "__"))] {
                                "[go]"
                            }
                        }
                    }
                }
            }
        }
    }

    Page<'a>(
        graph: &'a Graph,
        name: &'a str,
        dep_in: &'a HashMap<ComponentRef, Vec<Edge>>,
        dep_out: &'a HashMap<ComponentRef, Vec<Edge>>
    )  {
        {markup::doctype()}
        html {
            {Head {title: name}}
            body {
                h1 {
                    {name}
                }
                h2 { "Outgoing Dependencies" }
                {Deps {graph, deps: dep_out}}
                h2 { "Incoming Dependencies" }
                {Deps {graph, deps: dep_in}}
            }
        }
    }

    Head<'a>(title: &'a str) {
        head {
            title {
                {title}
            }
            style {
                {markup::raw(r#"
                body {
                    font-family: "Raleway", "HelveticaNeue", "Helvetica Neue", Helvetica, Arial, sans-serif;
                    margin: 2em;
                }
                ul {
                    font-family: monospace;
                }
                details {
                    font-family: monospace;
                    margin-bottom: 0.2em;
                }
                .dep-count {
                    color: #aaa;
                }
                "#)}
            }
        }
    }

    Deps<'a>(
        graph: &'a Graph,
        deps: &'a HashMap<ComponentRef, Vec<Edge>>
    ) {
        @for c_ref in sorted_dep_keys(&graph, &deps) {
            details {
                summary {
                    {graph.components[c_ref].nice_name()}
                    span[class="dep-count"] {
                        " ("
                        {deps[&c_ref].len()}
                        ") "
                    }
                    a[href=format!("{}.html", graph.components[c_ref].nice_name().replace('/', "__"))] {
                        "[go]"
                    }
                }
                ul {
                    @for e in &deps[&c_ref] {
                        li {
                            {graph.files[e.from].path}
                            " → "
                            {graph.files[e.to].path}
                        }
                    }
                }
            }
        }
    }
}
