//! truss-fem — a command-line finite element analyzer for 2D/3D trusses.
//!
//! A modern Rust reimplementation of the legacy MFC "CauFEM" program. The
//! original C++ engine only parsed input and drew a window; its solver was a
//! set of empty stubs. This version implements the full direct-stiffness method
//! and adds a self-contained web viewer for the structure and results.

mod linalg;
mod model;
mod report;
mod shell;
mod solver;
mod viewer;

use clap::{Parser, Subcommand};
use model::Model;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// A finite element analyzer for space/plane trusses (direct stiffness method).
#[derive(Parser)]
#[command(
    name = "truss-fem",
    version,
    about = "Finite element analysis for 2D/3D trusses, with a built-in web viewer.",
    long_about = "truss-fem analyzes pin-jointed truss structures using the direct \
stiffness method and reports nodal displacements, support reactions, and member \
axial forces/stresses. It can also emit an interactive, self-contained web viewer.",
    propagate_version = true
)]
struct Cli {
    /// With no subcommand, an interactive shell is started.
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the interactive shell (default when no subcommand is given).
    Shell,
    /// Analyze a model and print (or save) a results report.
    Solve {
        /// Path to the input model file.
        input: PathBuf,
        /// Output format.
        #[arg(short, long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Write the report to a file instead of stdout.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate an interactive web viewer (self-contained HTML) of the model and results.
    View {
        /// Path to the input model file.
        input: PathBuf,
        /// Output HTML path (default: <input>.html).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Open the generated file in the default browser.
        #[arg(long)]
        open: bool,
    },
    /// Serve the interactive viewer from a local web server and open it.
    Serve {
        /// Path to the input model file.
        input: PathBuf,
        /// Port to listen on.
        #[arg(short, long, default_value_t = 8080)]
        port: u16,
        /// Do not open the browser automatically.
        #[arg(long)]
        no_open: bool,
    },
    /// Show a summary of the model without running the analysis.
    Info {
        /// Path to the input model file.
        input: PathBuf,
    },
    /// Write a ready-to-run example input file to get started.
    Example {
        /// Destination path (default: truss_example.txt).
        #[arg(default_value = "truss_example.txt")]
        output: PathBuf,
    },
}

#[derive(Copy, Clone, clap::ValueEnum)]
enum Format {
    Text,
    Json,
}

pub const EXAMPLE_INPUT: &str = "SPACE TRUSS EXAMPLE OF SECTION\n\
3,4\n\
1,0,1,0,72.0,0.,0.,0.,0.,-1000.0\n\
2,1,1,1,0.0,36.0,0.,0.,0.,0.\n\
3,1,1,1,0.0,36.0,72.0,0.,0.,0.\n\
4,1,1,1,0.0,0.0,-48.0,0.,0.,0.\n\
1,1,4,1.2E+6,0.187\n\
2,1,2,1.2E+6,0.302\n\
3,1,3,1.2E+6,0.726\n";

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        None | Some(Command::Shell) => shell::run(),
        Some(Command::Solve { input, format, output }) => cmd_solve(&input, format, output.as_deref()),
        Some(Command::View { input, output, open }) => cmd_view(&input, output.as_deref(), open),
        Some(Command::Serve { input, port, no_open }) => cmd_serve(&input, port, !no_open),
        Some(Command::Info { input }) => cmd_info(&input),
        Some(Command::Example { output }) => cmd_example(&output),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

/// Load and analyze a model, mapping every failure to a friendly message.
fn load_and_solve(input: &Path) -> Result<(Model, solver::Solution), String> {
    let model = Model::from_file(input).map_err(|e| e.to_string())?;
    let sol = solver::solve(&model).map_err(|e| e.to_string())?;
    Ok((model, sol))
}

fn cmd_solve(input: &Path, format: Format, output: Option<&Path>) -> Result<(), String> {
    let (model, sol) = load_and_solve(input)?;
    let text = match format {
        Format::Text => report::text_report(&model, &sol),
        Format::Json => report::json_report(&model, &sol),
    };
    match output {
        Some(path) => {
            std::fs::write(path, &text)
                .map_err(|e| format!("cannot write '{}': {}", path.display(), e))?;
            println!("Report written to {}", path.display());
        }
        None => print!("{text}"),
    }
    Ok(())
}

fn cmd_view(input: &Path, output: Option<&Path>, open: bool) -> Result<(), String> {
    let (model, sol) = load_and_solve(input)?;
    let json = report::json_report(&model, &sol);
    let html = viewer::build_html(&sol.title, &json);

    let out_path: PathBuf = match output {
        Some(p) => p.to_path_buf(),
        None => with_extension(input, "html"),
    };
    std::fs::write(&out_path, &html)
        .map_err(|e| format!("cannot write '{}': {}", out_path.display(), e))?;
    println!("Viewer written to {}", out_path.display());

    if open {
        viewer::open_in_browser(&absolute_display_path(&out_path));
    }
    Ok(())
}

/// Absolute path as a plain string the OS shell can open.
///
/// `std::fs::canonicalize` on Windows returns a verbatim path (`\\?\C:\...`)
/// that `cmd /C start` refuses to open — strip the prefix.
pub fn absolute_display_path(path: &Path) -> String {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let s = abs.to_string_lossy().into_owned();
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

fn cmd_serve(input: &Path, port: u16, open: bool) -> Result<(), String> {
    let (model, sol) = load_and_solve(input)?;
    let json = report::json_report(&model, &sol);
    let html = viewer::build_html(&sol.title, &json);

    println!("truss-fem viewer  '{}'", sol.title);
    if open {
        viewer::open_in_browser(&format!("http://127.0.0.1:{port}"));
    }
    viewer::serve(html, port).map_err(|e| format!("server error on port {port}: {e}"))
}

fn cmd_info(input: &Path) -> Result<(), String> {
    let model = Model::from_file(input).map_err(|e| e.to_string())?;
    let dim = if model.is_planar() { "2D (planar)" } else { "3D (space)" };
    let supports = model
        .nodes
        .iter()
        .filter(|n| n.fixed.iter().any(|&f| f))
        .count();
    let loaded = model
        .nodes
        .iter()
        .filter(|n| n.force.iter().any(|&f| f.abs() > 0.0))
        .count();

    println!("Model    : {}", model.title);
    println!("Type     : {dim} truss");
    println!("Nodes    : {}", model.nodes.len());
    println!("Elements : {}", model.elements.len());
    println!("Supports : {supports} node(s) with at least one fixed DOF");
    println!("Loads    : {loaded} node(s) with an applied force");
    println!("DOFs     : {} ({} free)", model.dof_count(), free_dofs(&model));
    Ok(())
}

fn free_dofs(model: &Model) -> usize {
    model
        .nodes
        .iter()
        .map(|n| n.fixed.iter().filter(|&&f| !f).count())
        .sum()
}

fn cmd_example(output: &Path) -> Result<(), String> {
    std::fs::write(output, EXAMPLE_INPUT)
        .map_err(|e| format!("cannot write '{}': {}", output.display(), e))?;
    println!("Example model written to {}", output.display());
    println!("Try:  truss-fem solve {}", output.display());
    println!("      truss-fem view  {} --open", output.display());
    Ok(())
}

/// Replace (or append) a file extension, keeping the original stem.
fn with_extension(path: &Path, ext: &str) -> PathBuf {
    let mut p = path.to_path_buf();
    p.set_extension(ext);
    p
}
