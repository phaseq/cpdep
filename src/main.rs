use structopt::StructOpt;

mod cli;
mod file_collector;
mod graph;
mod ui;

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
    Component {
        /// show incoming and outgoing links for this component
        component_from: Option<String>,

        // giving a second component restricts links further
        component_to: Option<String>,

        /// show files for dependencies
        #[structopt(long, short)]
        verbose: bool,

        #[structopt(long)]
        only_public: bool,
    },
    /// show which headers are public and which are private
    Headers {
        component: String,
        #[structopt(long, short)]
        verbose: bool,
    },
    /// show incoming and outgoing links for the given file
    File { file_name: String },
    /// show terminal UI
    UI {},
    /// show all strongly connected components
    Scc {},
    /// list the shortest path from component A to B
    Shortest {
        component_from: String,
        component_to: String,
        #[structopt(long, short)]
        verbose: bool,

        // only list paths reachable via public header files of A
        #[structopt(long)]
        only_public: bool,
    },
}

fn main() -> Result<(), failure::Error> {
    let options = Opt::from_args();
    let graph = graph::load(&options);

    match options.cmd {
        Cmd::Component {
            component_from,
            component_to,
            verbose,
            only_public,
        } => cli::print_components(&graph, component_from, component_to, verbose, only_public),
        Cmd::File { file_name } => cli::print_file_info(&graph, &file_name),
        Cmd::Headers { component, verbose } => cli::print_headers(&graph, component, verbose),
        Cmd::UI {} => ui::show_ui(&graph)?,
        Cmd::Scc {} => cli::show_sccs(&graph),
        Cmd::Shortest {
            component_from,
            component_to,
            verbose,
            only_public,
        } => cli::print_shortest(&graph, &component_from, &component_to, verbose, only_public),
    }

    Ok(())
}
