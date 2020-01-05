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

    /// warn about malformed includes
    //#[structopt(long)]
    //warn_malformed: bool,

    /// show files for incoming dependencies
    #[structopt(long)]
    show_incoming: bool,

    /// show files for outgoing dependencies
    #[structopt(long)]
    show_outgoing: bool,
}

/*
func main() {
    project := read_files(*root_dir, flags)
    project.assign_files_to_components()
    project.generate_file_deps(flags)
    project.print_components(flags)
    //project.dbg_files()
}*/

fn main() -> io::Result<()> {
    let opt = Opt::from_args();
    let mut project = read_files(&opt.root)?;
    project.assign_files_to_components();
    project.generate_file_deps(&opt);
    project.print_components(&opt);

    //println!("{:?}", project);
    Ok(())
}

type ComponentRef = usize;
type FileRef = usize;

/*type file struct {
    path          string
    include_paths []string

    component      *component
    incoming_links []*file
    outgoing_links []*file
}*/
#[derive(Debug)]
struct File {
    path: String,
    include_paths: Vec<String>,

    component: Option<ComponentRef>,
    incoming_links: Vec<FileRef>,
    outgoing_links: Vec<FileRef>,
}

/*func (f *file) print() {
    fmt.Printf("%s\n", f.path)
    fmt.Printf("  Component: %s\n", f.component.nice_name())

    fmt.Println("  Includes:")
    for _, include := range f.include_paths {
        fmt.Printf("    %s\n", include)
    }

    fmt.Println("  Incoming:")
    for _, fo := range f.incoming_links {
        fmt.Printf("    %s\n", fo.path)
    }

    fmt.Println("  Outgoing:")
    for _, fo := range f.outgoing_links {
        fmt.Printf("    %s\n", fo.path)
    }
}*/
/*impl File {
    fn print(&self, project: &Project) {
        println!("{}", self.path);
        println!(
            "  Component: {}",
            project.component(self.component.unwrap()).path
        );

        // TODO
    }
}*/

/*type component struct {
    path  string
    files []*file
}*/
#[derive(Debug)]
struct Component {
    path: String,
    files: Vec<FileRef>,
}

/*func (c *component) nice_name() string {
    if c.path == "" {
        return "."
    }
    return c.path
}*/
impl Component {
    fn nice_name(&self) -> &str {
        if self.path.is_empty() {
            return ".";
        }
        return &self.path;
    }
}

/*type dependency struct {
    component *component
    edges     []edge
}*/
/*struct Dependency {
    component: ComponentRef,
    edges: Vec<Edge>,
}*/

/*type edge struct {
    from *file
    to   *file
}*/
struct Edge {
    from: FileRef,
    to: FileRef,
}

/*type project struct {
    root       string
    files      []file
    components []component
}*/
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

    /*func (p *project) rel_path(path string) string {
        rel_path := strings.TrimPrefix(strings.TrimPrefix(path, p.root), "/")
        return rel_path
    }*/
    fn rel_path<'a>(&self, path: &'a str) -> &'a str {
        return path.trim_start_matches(&self.root).trim_start_matches('/');
    }

    /*func (p *project) print_components(flags log_flags) {
        for _, c := range p.components {
            should_print := len(flags.components) == 0
            for _, name := range flags.components {
                if name == c.nice_name() {
                    should_print = true
                    break
                }
            }
            if should_print {
                c.print(flags)
            }
        }
    }*/
    fn print_components(&self, options: &Opt) {
        for (c_ref, c) in self.components.iter().enumerate() {
            let c_name = c.nice_name();
            if options.components.is_empty() || options.components.iter().any(|x| x == c_name) {
                self.print_component(c_ref, options);
            }
        }
    }

    /*func (c *component) print(flags log_flags) {
        fmt.Printf("%s (%d)\n", c.nice_name(), len(c.files))

        in, out := c.linked_components()
        sort.Slice(in, func(i, j int) bool {
            return in[i].component.path < in[j].component.path
        })
        sort.Slice(out, func(i, j int) bool {
            return out[i].component.path < out[j].component.path
        })

        fmt.Println("  Incoming:")
        for _, dep := range in {
            fmt.Printf("    %s\n", dep.component.nice_name())
            if flags.show_incoming {
                for _, e := range dep.edges {
                    fmt.Printf("      %s -> %s\n", e.from.path, e.to.path)
                }
            }
        }

        fmt.Println("  Outgoing:")
        for _, dep := range out {
            fmt.Printf("    %s\n", dep.component.nice_name())
            if flags.show_outgoing {
                for _, e := range dep.edges {
                    fmt.Printf("      %s -> %s\n", e.from.path, e.to.path)
                }
            }
        }

        fmt.Println("  Files:")
        for _, f := range c.files {
            fmt.Printf("   %s\n", f.path)
        }
    }*/
    fn print_component(&self, c: ComponentRef, options: &Opt) {
        println!("{} ({})", self.component(c).nice_name(), self.files.len());

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

        /*println!("  Files:");
        for f in self.component(c).files.iter() {
            println!("    {}", self.file(*f).path);
        }*/
    }

    /*func (p *project) print_files() {
        for _, f := range p.files {
            f.print()
        }
    }*/
    /*fn print_files(&self) {
        for f in self.files.iter() {
            f.print(&self);
        }
    }*/

    /*func (c *component) linked_components() ([]dependency, []dependency) {
        incoming := make(map[*component][]edge)
        outgoing := make(map[*component][]edge)
        for f_index, f := range c.files {
            for _, in := range f.incoming_links {
                if in.component.path != c.path {
                    incoming[in.component] = append(
                        incoming[in.component], edge{from: in, to: c.files[f_index]})
                }
            }
            for _, out := range f.outgoing_links {
                if out.component.path != c.path {
                    outgoing[out.component] = append(
                        outgoing[out.component], edge{from: c.files[f_index], to: out})
                }
            }
        }
        in := make([]dependency, 0, len(incoming))
        for k := range incoming {
            in = append(in, dependency{component: k, edges: incoming[k]})
        }
        out := make([]dependency, 0, len(outgoing))
        for k := range outgoing {
            out = append(out, dependency{component: k, edges: outgoing[k]})
        }
        return in, out
    }*/
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

    /*func (p *project) assign_files_to_components() {
        for i_file, file := range p.files {
            // Iterate over prefixes of file path, to find the most specific component.
            // Assign to the most specific component.
            // Example: file path: "a/b/header.hpp"
            // candidate 1: a/b
            // candidate 2: a
            // candidate 3: ''
            idx := 0
            path := file.path
            for idx != -1 {
                idx = strings.LastIndex(path, "/")
                if idx != -1 {
                    path = path[:idx]
                } else {
                    path = ""
                }
                for i_c, c := range p.components {
                    if c.path == path {
                        p.components[i_c].files = append(c.files, &p.files[i_file])
                        p.files[i_file].component = &p.components[i_c]
                        idx = -1
                        break
                    }
                }
            }
        }
    }*/
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

    /*func (p *project) generate_file_deps(flags log_flags) {
        // map from possible include paths to corresponding files
        // for example: "a/b/header.h" could be included as "header.h", "b/header.h", and "a/b/header.h"
        // assumption here: normalized paths with unix slashes
        path_to_files := make(map[string][]*file)
        for i_file, file := range p.files {
            path := file.path
            path_to_files[path] = append(path_to_files[path], &p.files[i_file])
            for idx := strings.Index(path, "/"); idx != -1; idx = strings.Index(path, "/") {
                path = path[idx+1:]
                path_to_files[path] = append(path_to_files[path], &p.files[i_file])
            }
        }

        for i_file, file := range p.files {
            for _, include := range file.include_paths {
                deps, present := path_to_files[include]
                if present {
                    // If a file can be included from the current solution, assume that it is.
                    // This avoids adding dependencies to headers with name clashes (like StdAfx.h).
                    is_present_in_this_component := false
                    for _, dep := range deps {
                        if dep.component == file.component {
                            is_present_in_this_component = true
                            break
                        }
                    }
                    if !is_present_in_this_component {
                        for _, dep := range deps {
                            p.files[i_file].outgoing_links =
                                append(p.files[i_file].outgoing_links, dep)

                            dep.incoming_links =
                                append(dep.incoming_links, &p.files[i_file])
                        }
                    }
                } else if flags.warn_missing {
                    fmt.Printf("Include not found in %s: %s\n", file.path, include)
                }
            }
        }
    }*/

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

/*func read_files(root_path string, flags log_flags) project {
    source_suffixes := []string{".cpp", ".hpp", ".c", ".h"}
    ignore_patterns := []string{".svn", "dev/tools"}

    root_path = strings.TrimSuffix(root_path, "/")

    project := project{root: root_path}

    err := filepath.Walk(project.root, func(path string, info os.FileInfo, err error) error {
        if err != nil {
            fmt.Printf("prevent panic by handling failure accessing a path %q: %v\n", path, err)
            return err
        }
        for _, pattern := range ignore_patterns {
            if strings.Contains(path, pattern) {
                fmt.Printf("skipping: %s\n", path)
                return filepath.SkipDir
            }
        }
        if info.Name() == "CMakeLists.txt" {
            component_path := project.rel_path(strings.TrimSuffix(path, "/CMakeLists.txt"))
            project.components = append(project.components, component{path: component_path})
        }
        for _, suffix := range source_suffixes {
            if strings.HasSuffix(path, suffix) {
                include_paths := extract_includes(path, flags)
                new_file := file{path: project.rel_path(path), include_paths: include_paths}
                project.files = append(project.files, new_file)
            }
        }
        return nil
    })
    if err != nil {
        fmt.Printf("error walking the path %q: %v\n", project.root, err)
        panic(err)
    }
    return project
}*/
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

/*func extract_includes(path string, flags log_flags) []string {
    fh, err := os.Open(path)
    check(err)
    defer fh.Close()

    var results []string

    r := bufio.NewScanner(bufio.NewReader(fh))
    for r.Scan() {
        if strings.HasPrefix(r.Text(), "#include") {
            line := r.Text()
            iStart := strings.IndexAny(line, "\"<")
            iEnd := strings.LastIndexAny(line, "\">")
            if iStart == -1 || iEnd == -1 || iStart >= iEnd {
                if flags.warn_malformed {
                    fmt.Printf("malformed #include in %s: %s\n", path, line)
                }
                continue
            }
            include_path := line[(iStart + 1):iEnd]
            if strings.Contains(include_path, "\\") || strings.Contains(include_path, "..") {
                if flags.warn_malformed {
                    fmt.Printf("malformed #include in %s: %s\n", path, include_path)
                }
                continue
            }
            results = append(results, include_path)
        }
    }

    return results
}*/
fn extract_includes(path: &Path) -> io::Result<Vec<String>> {
    let mut results = Vec::new();
    let mut f = std::fs::File::open(path)?;
    let mut c = Vec::new();
    f.read_to_end(&mut c)?;
    //let mut line = String::new();
    //while r.read_line(&mut line)? != 0 {
    /*for line in r.lines() {
        let line = line?;
        if !line.starts_with("#include") {
            continue; // TODO: handle "#   include  <header.h> // <>?"
        }
        let i_start = line.find(|c| c == '"' || c == '<');
        let i_end = line.rfind(|c| c == '"' || c == '>');
        if i_start.is_none() || i_end.is_none() || i_start.unwrap() >= i_end.unwrap() {
            if options.warn_malformed {
                println!("malformed #include in {:?}: {}", path, line);
            }
            continue;
        }
        let include_path = &line[(i_start.unwrap() + 1)..i_end.unwrap()];
        if include_path.contains('\\') || include_path.contains("..") {
            if options.warn_malformed {
                println!("malformed #include in {:?}: {}", path, line);
            }
            continue;
        }
        results.push(include_path.into());
    }
    Ok(results)*/
    let re = regex::bytes::Regex::new("#include [<\"]([^>\"]+)[>\"]").unwrap();
    for cap in re.captures_iter(&c) {
        results.push(String::from_utf8_lossy(&cap[1]).into());
    }
    Ok(results)
}

/*func check(e error) {
    if e != nil {
        panic(e)
    }
}*/
