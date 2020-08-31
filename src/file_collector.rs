use ignore::{DirEntry, ParallelVisitor, ParallelVisitorBuilder, WalkState};
use lazy_static::lazy_static;
use std::io::{self, Read};
use std::path::Path;
use std::sync::{Arc, Mutex};

lazy_static! {
    static ref INCLUDE_RE: regex::bytes::Regex =
        regex::bytes::Regex::new("#\\s*include\\s*[<\"]([^>\"]+)").unwrap();
    static ref INCLUDE_RE_16: regex::bytes::Regex =
        regex::bytes::Regex::new("#\0[\\s\0]*i\0n\0c\0l\0u\0d\0e\0[\\s\0]*[<\"]\0([^>\"]+)")
            .unwrap();
}

pub fn read_files(options: &crate::Opt) -> FileCollector {
    let root_path = options.root.replace('\\', "/");
    let root_path = root_path.trim_end_matches('/');

    let collector = Arc::new(Mutex::new(FileCollector {
        files: vec![],
        components: vec![],
    }));

    let mut builder = FileCollectorBuilder {
        root: root_path.to_owned(),
        warn_malformed: options.warn_malformed,
        file_collector: collector,
    };

    ignore::WalkBuilder::new(root_path.to_owned())
        .threads(6)
        .build_parallel()
        .visit(&mut builder);

    let lock = std::sync::Arc::try_unwrap(builder.file_collector).unwrap();
    let mut base_project = lock.into_inner().unwrap();
    if base_project
        .components
        .iter()
        .find(|c| c.path.is_empty())
        .is_none()
    {
        base_project.components.push(Component {
            path: String::new(),
        });
    }
    base_project
}

#[derive(Debug)]
pub struct FileCollector {
    pub files: Vec<File>,
    pub components: Vec<Component>,
}

#[derive(Debug)]
pub struct File {
    pub path: String,
    pub include_paths: Vec<String>,
}

#[derive(Debug)]
pub struct Component {
    pub path: String,
}

impl Component {
    pub fn nice_name(&self) -> &str {
        if self.path.is_empty() {
            return ".";
        }
        &self.path
    }
}

struct FileCollectorBuilder {
    root: String,
    warn_malformed: bool,
    file_collector: Arc<Mutex<FileCollector>>,
}

impl<'a, 's> ParallelVisitorBuilder<'s> for FileCollectorBuilder {
    fn build(&mut self) -> Box<dyn ignore::ParallelVisitor + 's> {
        Box::new(FileCollectorThread {
            root: self.root.clone(),
            warn_malformed: self.warn_malformed,
            files: vec![],
            components: vec![],
            parent: self.file_collector.clone(),
        })
    }
}

struct FileCollectorThread {
    root: String,
    warn_malformed: bool,
    files: Vec<File>,
    components: Vec<Component>,
    parent: Arc<Mutex<FileCollector>>,
}

impl FileCollectorThread {
    fn rel_path<'a>(&self, path: &'a str) -> &'a str {
        path.trim_start_matches(&self.root).trim_start_matches('/')
    }
}

impl Drop for FileCollectorThread {
    fn drop(&mut self) {
        let mut parent = self.parent.lock().unwrap();
        parent.files.append(&mut self.files);
        parent.components.append(&mut self.components);
    }
}

impl ParallelVisitor for FileCollectorThread {
    fn visit(&mut self, entry: Result<DirEntry, ignore::Error>) -> WalkState {
        let source_suffixes = [
            ".cpp", ".hpp", ".c", ".h", ".inl", ".hh", ".cc", ".ipp", ".imp", ".impl", ".H",
        ];
        match entry {
            Ok(entry) => {
                let path_str = entry
                    .path()
                    .to_str()
                    .expect("failed to parse file name")
                    .replace('\\', "/");
                if entry.path().ends_with("CMakeLists.txt") {
                    let path = path_str.trim_end_matches("/CMakeLists.txt");
                    let path = self.rel_path(path).to_string();
                    self.components.push(Component { path });
                } else if source_suffixes.iter().any(|s| path_str.ends_with(s)) {
                    match extract_includes(&entry.path(), self.warn_malformed) {
                        Ok(include_paths) => {
                            let path = self.rel_path(&path_str).to_string();
                            self.files.push(File {
                                path,
                                include_paths,
                            })
                        }
                        Err(e) => println!("Error while parsing {}: {}", path_str, e),
                    }
                }
            }
            Err(e) => {
                println!("Failed to parse file: {}", e);
            }
        }
        WalkState::Continue
    }
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
