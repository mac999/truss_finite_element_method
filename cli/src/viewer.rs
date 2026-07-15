//! Self-contained web viewer generation and a dependency-free local server.
//!
//! [`build_html`] injects the analysed model (as JSON) into an HTML template
//! that renders an interactive 3D view using only vanilla JavaScript — no CDN,
//! no build step, works offline by double-clicking the file.
//!
//! [`serve`] spins up a tiny std-only HTTP server so `truss-fem serve` can host
//! the viewer and open it in the browser.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

const TEMPLATE: &str = include_str!("viewer_template.html");

/// Produce a complete, standalone HTML document for the given model JSON.
pub fn build_html(title: &str, model_json: &str) -> String {
    // The template carries two placeholders. Replace the data first so a `{`
    // inside JSON can never collide with the title substitution.
    TEMPLATE
        .replace("__DATA__", model_json)
        .replace("__TITLE__", &html_escape(title))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Serve a single HTML page on `127.0.0.1:<port>` until the process is killed.
///
/// Any request path returns the same page (a single-page app). Returns the
/// bound address so the caller can print/open it.
pub fn serve(html: String, port: u16) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let addr = listener.local_addr()?;
    println!("  Serving viewer at http://{addr}");
    println!("  Press Ctrl+C to stop.");

    let body = html.into_bytes();
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                // Best-effort; a failed client connection must not stop the server.
                let _ = handle_client(s, &body);
            }
            Err(e) => eprintln!("  connection error: {e}"),
        }
    }
    Ok(())
}

fn handle_client(mut stream: TcpStream, body: &[u8]) -> std::io::Result<()> {
    // Drain the request line/headers (we don't need them, but reading avoids
    // resets on some clients).
    let mut buf = [0u8; 1024];
    let _ = stream.read(&mut buf);

    let header = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// Try to open a URL or file in the default browser (best effort, per-OS).
pub fn open_in_browser(target: &str) {
    #[cfg(target_os = "windows")]
    let cmd = std::process::Command::new("cmd")
        .args(["/C", "start", "", target])
        .spawn();
    #[cfg(target_os = "macos")]
    let cmd = std::process::Command::new("open").arg(target).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = std::process::Command::new("xdg-open").arg(target).spawn();

    if let Err(e) = cmd {
        eprintln!("  (could not auto-open browser: {e})");
    }
}
