//! Interactive shell (REPL) for truss-fem.
//!
//! Launched by running `truss-fem` with no subcommand (or `truss-fem shell`).
//! Keeps a *current model* in memory so you can load once and then inspect,
//! solve, view, and save without retyping the path:
//!
//! ```text
//! truss-fem> load input/bridge_pratt_2d.txt
//! truss-fem> solve
//! truss-fem> view
//! truss-fem> save results.json
//! ```

use crate::model::Model;
use crate::solver::{self, Solution};
use crate::{report, viewer};
use std::io::{self, BufRead, Write as _};
use std::path::{Path, PathBuf};

/// Everything the shell remembers between commands.
#[derive(Default)]
struct State {
    /// Path the current model was loaded from.
    path: Option<PathBuf>,
    model: Option<Model>,
    /// Analysis result for `model` (cleared whenever a new model is loaded).
    solution: Option<Solution>,
}

/// Run the interactive shell until `exit` or EOF. Never returns an error to
/// the caller for a bad command — those are printed and the loop continues.
pub fn run() -> Result<(), String> {
    print_banner();

    let stdin = io::stdin();
    let mut state = State::default();

    loop {
        print!("truss-fem> ");
        io::stdout().flush().ok();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF (Ctrl+Z / Ctrl+D)
            Ok(_) => {}
            Err(e) => return Err(format!("cannot read input: {e}")),
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let (cmd, arg) = split_command(line);
        let result = match cmd.to_ascii_lowercase().as_str() {
            "help" | "?" => {
                print_help();
                Ok(())
            }
            "exit" | "quit" | "q" => break,
            "load" => state.load(arg),
            "info" => state.info(arg),
            "solve" | "run" => state.solve(arg),
            "report" => state.report(arg),
            "save" => state.save(arg),
            "save-model" | "savemodel" => state.save_model(arg),
            "view" => state.view(arg),
            "serve" => state.serve(arg),
            "samples" | "ls" => {
                list_samples();
                Ok(())
            }
            "example" => cmd_example(arg),
            "clear" | "cls" => {
                print!("\x1b[2J\x1b[H");
                io::stdout().flush().ok();
                Ok(())
            }
            other => Err(format!("unknown command '{other}' — type 'help' for a list")),
        };
        if let Err(msg) = result {
            println!("error: {msg}");
        }
    }

    println!("bye.");
    Ok(())
}

/// Split a command line into the command word and the (possibly empty) rest.
/// The rest keeps internal spaces so paths with spaces work; surrounding
/// quotes are stripped.
fn split_command(line: &str) -> (&str, &str) {
    match line.find(char::is_whitespace) {
        Some(pos) => (&line[..pos], unquote(line[pos..].trim())),
        None => (line, ""),
    }
}

fn unquote(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2 && (s.starts_with('"') && s.ends_with('"') || s.starts_with('\'') && s.ends_with('\'')) {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn print_banner() {
    println!("truss-fem v{} — interactive shell", env!("CARGO_PKG_VERSION"));
    println!("Finite element analysis for 2D/3D trusses (direct stiffness method).");
    println!();
    println!("Type 'help' for commands, 'samples' to list bundled models, 'exit' to quit.");
    println!("Quick start:  load input/bridge_pratt_2d.txt   then:  solve   view");
    println!();
}

fn print_help() {
    println!("Commands:");
    println!("  load <file>            Load a truss model from an input file.");
    println!("  info [file]            Model summary (loads the file first if given).");
    println!("  solve [file]           Run the FEM analysis and print the report.");
    println!("  report [text|json]     Re-print the last report in the given format.");
    println!("  save <file>            Save the last report ('.json' ext -> JSON, else text).");
    println!("  save-model <file>      Save the current model in the input file format.");
    println!("  view [out.html]        Solve if needed, write an HTML viewer, open browser.");
    println!("  serve [port]           Host the viewer at http://127.0.0.1:<port> (default 8080).");
    println!("  samples                List bundled sample models (input/, examples/).");
    println!("  example <file>         Write a ready-to-run example input file.");
    println!("  clear                  Clear the screen.");
    println!("  help                   Show this help.");
    println!("  exit                   Leave the shell.");
    println!();
    println!("Tip: 'solve <file>' and 'view <file>' also accept a path directly.");
}

/// Directories searched by `samples`, relative to the current directory.
const SAMPLE_DIRS: [&str; 2] = ["input", "examples"];

fn list_samples() {
    let mut found_any = false;
    for dir in SAMPLE_DIRS {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map_or(false, |x| x == "txt"))
            .collect();
        files.sort();
        if files.is_empty() {
            continue;
        }
        found_any = true;
        println!("{dir}/");
        for f in files {
            match Model::from_file(&f) {
                Ok(m) => println!(
                    "  {:<38} {} nodes, {} elements — {}",
                    f.display(),
                    m.nodes.len(),
                    m.elements.len(),
                    m.title
                ),
                Err(_) => println!("  {}", f.display()),
            }
        }
    }
    if !found_any {
        println!("no sample .txt files found under ./input or ./examples");
        println!("(start the shell from the project root, or use 'example my_truss.txt')");
    }
}

fn cmd_example(arg: &str) -> Result<(), String> {
    let path = if arg.is_empty() { "truss_example.txt" } else { arg };
    std::fs::write(path, crate::EXAMPLE_INPUT)
        .map_err(|e| format!("cannot write '{path}': {e}"))?;
    println!("Example model written to {path}");
    println!("Try:  load {path}   then:  solve   view");
    Ok(())
}

impl State {
    /// Borrow the loaded model or explain how to get one.
    fn require_model(&self) -> Result<&Model, String> {
        self.model
            .as_ref()
            .ok_or_else(|| "no model loaded — use 'load <file>' first (see 'samples')".into())
    }

    /// Load `arg` if given; afterwards a model must be present.
    fn load_if_given(&mut self, arg: &str) -> Result<(), String> {
        if !arg.is_empty() {
            self.load(arg)?;
        }
        self.require_model().map(|_| ())
    }

    /// Make sure `solution` matches the current model, solving on demand.
    fn ensure_solved(&mut self) -> Result<(), String> {
        if self.solution.is_none() {
            let model = self.require_model()?;
            let sol = solver::solve(model).map_err(|e| e.to_string())?;
            self.solution = Some(sol);
        }
        Ok(())
    }

    fn load(&mut self, arg: &str) -> Result<(), String> {
        if arg.is_empty() {
            return Err("usage: load <file>".into());
        }
        let path = PathBuf::from(arg);
        let model = Model::from_file(&path).map_err(|e| e.to_string())?;
        println!(
            "Loaded '{}' — {} nodes, {} elements ({}).",
            model.title,
            model.nodes.len(),
            model.elements.len(),
            if model.is_planar() { "2D planar" } else { "3D space" },
        );
        self.model = Some(model);
        self.path = Some(path);
        self.solution = None; // stale for the new model
        Ok(())
    }

    fn info(&mut self, arg: &str) -> Result<(), String> {
        self.load_if_given(arg)?;
        let model = self.require_model()?;
        let dim = if model.is_planar() { "2D (planar)" } else { "3D (space)" };
        let supports = model.nodes.iter().filter(|n| n.fixed.iter().any(|&f| f)).count();
        let loaded = model
            .nodes
            .iter()
            .filter(|n| n.force.iter().any(|&f| f.abs() > 0.0))
            .count();
        let free: usize = model
            .nodes
            .iter()
            .map(|n| n.fixed.iter().filter(|&&f| !f).count())
            .sum();

        if let Some(p) = &self.path {
            println!("File     : {}", p.display());
        }
        println!("Model    : {}", model.title);
        println!("Type     : {dim} truss");
        println!("Nodes    : {}", model.nodes.len());
        println!("Elements : {}", model.elements.len());
        println!("Supports : {supports} node(s) with at least one fixed DOF");
        println!("Loads    : {loaded} node(s) with an applied force");
        println!("DOFs     : {} ({} free)", model.dof_count(), free);
        Ok(())
    }

    fn solve(&mut self, arg: &str) -> Result<(), String> {
        self.load_if_given(arg)?;
        self.solution = None; // force a fresh run
        self.ensure_solved()?;
        let (model, sol) = (self.model.as_ref().unwrap(), self.solution.as_ref().unwrap());
        print!("{}", report::text_report(model, sol));
        Ok(())
    }

    fn report(&mut self, arg: &str) -> Result<(), String> {
        self.ensure_solved()?;
        let (model, sol) = (self.model.as_ref().unwrap(), self.solution.as_ref().unwrap());
        match arg.to_ascii_lowercase().as_str() {
            "" | "text" => print!("{}", report::text_report(model, sol)),
            "json" => print!("{}", report::json_report(model, sol)),
            other => return Err(format!("unknown format '{other}' (expected 'text' or 'json')")),
        }
        Ok(())
    }

    fn save(&mut self, arg: &str) -> Result<(), String> {
        if arg.is_empty() {
            return Err("usage: save <file>   ('.json' extension saves JSON, otherwise text)".into());
        }
        self.ensure_solved()?;
        let (model, sol) = (self.model.as_ref().unwrap(), self.solution.as_ref().unwrap());
        let path = Path::new(arg);
        let is_json = path
            .extension()
            .map_or(false, |x| x.eq_ignore_ascii_case("json"));
        let text = if is_json {
            report::json_report(model, sol)
        } else {
            report::text_report(model, sol)
        };
        std::fs::write(path, &text).map_err(|e| format!("cannot write '{arg}': {e}"))?;
        println!(
            "{} report saved to {arg}",
            if is_json { "JSON" } else { "Text" }
        );
        Ok(())
    }

    fn save_model(&mut self, arg: &str) -> Result<(), String> {
        if arg.is_empty() {
            return Err("usage: save-model <file>".into());
        }
        let model = self.require_model()?;
        std::fs::write(arg, model.to_input_string())
            .map_err(|e| format!("cannot write '{arg}': {e}"))?;
        println!("Model saved to {arg} (truss-fem input format)");
        Ok(())
    }

    /// Build the standalone HTML viewer and open it in the default browser.
    fn view(&mut self, arg: &str) -> Result<(), String> {
        // `view <model.txt>` loads then views; `view <out.html>` sets the output.
        let mut out_path: Option<PathBuf> = None;
        if !arg.is_empty() {
            let p = Path::new(arg);
            if p.extension().map_or(false, |x| x.eq_ignore_ascii_case("html")) {
                out_path = Some(p.to_path_buf());
            } else {
                self.load(arg)?;
            }
        }
        self.ensure_solved()?;
        let (model, sol) = (self.model.as_ref().unwrap(), self.solution.as_ref().unwrap());

        let out = out_path.unwrap_or_else(|| match &self.path {
            Some(p) => p.with_extension("html"),
            None => PathBuf::from("truss_view.html"),
        });
        let html = viewer::build_html(&sol.title, &report::json_report(model, sol));
        std::fs::write(&out, &html).map_err(|e| format!("cannot write '{}': {e}", out.display()))?;
        println!("Viewer written to {}", out.display());

        let abs = crate::absolute_display_path(&out);
        println!("Opening in browser: {abs}");
        viewer::open_in_browser(&abs);
        Ok(())
    }

    /// Serve the viewer on a background thread so the shell stays usable.
    fn serve(&mut self, arg: &str) -> Result<(), String> {
        let port: u16 = if arg.is_empty() {
            8080
        } else {
            arg.parse().map_err(|_| format!("invalid port '{arg}'"))?
        };
        self.ensure_solved()?;
        let (model, sol) = (self.model.as_ref().unwrap(), self.solution.as_ref().unwrap());
        let html = viewer::build_html(&sol.title, &report::json_report(model, sol));

        std::thread::spawn(move || {
            if let Err(e) = viewer::serve(html, port) {
                println!("serve error on port {port}: {e}");
            }
        });
        // Give the listener a moment to bind (or fail) before opening.
        std::thread::sleep(std::time::Duration::from_millis(150));
        viewer::open_in_browser(&format!("http://127.0.0.1:{port}"));
        println!("(the server keeps running in the background; 'exit' stops it)");
        Ok(())
    }
}
