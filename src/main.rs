mod filter;

use std::{convert::Infallible, io::IsTerminal, sync::Arc};

use axum::{
    body::{Body, Bytes},
    extract::{RawQuery, State},
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse, Redirect, Response,
    },
    routing::get,
    Router,
};
use clap::{ArgGroup, CommandFactory, Parser};
use filter::{LineFilter, MatchOptions};
use futures_util::stream::{self, Stream, StreamExt};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::{broadcast, Mutex},
};
use tokio_stream::wrappers::BroadcastStream;

/// How many lines are kept so late joiners still see recent output.
const HISTORY_LIMIT: usize = 10_000;

const AFTER_HELP: &str = "\
Examples:
  tail -f app.log | spiderpipe            view a log at http://localhost:3000
  ping example.com | spiderpipe -p 8080   use another port
  make 2>&1 | spiderpipe --remote         let other machines on the network watch
  tail -f app.log | spiderpipe ERROR -C3  keep only errors and 3 lines around each
  tail -f app.log | spiderpipe --web-regexp warn --web-ignore-case
                                          keep everything, open the page filtered to warn

Endpoints:
  /         live web view
  /raw      plain-text stream (curl -N http://localhost:3000/raw)
  /events   server-sent events, one event per line

The last 10000 lines are kept, so late viewers see recent output.
After the input ends the server keeps running until Ctrl+C.";

const FILTER: &str = "Filter (drops lines before they are stored, like grep)";
const WEB: &str = "Web page defaults (opening / redirects to a URL with these set; each viewer can change them)";

/// Stream whatever is piped into stdin to a web page.
#[derive(Parser)]
#[command(version, after_help = AFTER_HELP)]
#[command(group(ArgGroup::new("any_pattern").args(["pattern", "patterns"])))]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value_t = 3000)]
    port: u16,

    /// Allow access from other machines (bind 0.0.0.0 instead of 127.0.0.1)
    #[arg(short, long)]
    remote: bool,

    /// Keep only lines that match PATTERN, as in `grep PATTERN`
    #[arg(value_name = "PATTERN", help_heading = FILTER)]
    pattern: Option<String>,

    /// Use PATTERN for matching. Repeat to keep lines that match any of them
    #[arg(short = 'e', long = "regexp", value_name = "PATTERN", help_heading = FILTER)]
    patterns: Vec<String>,

    /// Accepted for grep compatibility. Patterns are always extended regular expressions
    #[arg(short = 'E', long, requires = "any_pattern", help_heading = FILTER)]
    extended_regexp: bool,

    /// Treat patterns as plain text, not regular expressions
    #[arg(short = 'F', long, requires = "any_pattern", help_heading = FILTER)]
    fixed_strings: bool,

    /// Ignore case when matching
    #[arg(short, long, requires = "any_pattern", help_heading = FILTER)]
    ignore_case: bool,

    /// Match only whole words
    #[arg(short, long, requires = "any_pattern", help_heading = FILTER)]
    word_regexp: bool,

    /// Match only whole lines
    #[arg(short = 'x', long, requires = "any_pattern", help_heading = FILTER)]
    line_regexp: bool,

    /// Keep the lines that do not match instead
    #[arg(short = 'v', long, requires = "any_pattern", help_heading = FILTER)]
    invert_match: bool,

    /// Also keep N lines after each match
    #[arg(short = 'A', long, value_name = "N", requires = "any_pattern", help_heading = FILTER)]
    after_context: Option<usize>,

    /// Also keep N lines before each match
    #[arg(short = 'B', long, value_name = "N", requires = "any_pattern", help_heading = FILTER)]
    before_context: Option<usize>,

    /// Also keep N lines before and after each match. -A and -B take precedence
    #[arg(short = 'C', long, value_name = "N", requires = "any_pattern", help_heading = FILTER)]
    context: Option<usize>,

    /// Pattern to put in the page's filter box
    #[arg(long, value_name = "PATTERN", help_heading = WEB)]
    web_regexp: Option<String>,

    /// Turn on the page's -F
    #[arg(long, help_heading = WEB)]
    web_fixed_strings: bool,

    /// Turn on the page's -i
    #[arg(long, help_heading = WEB)]
    web_ignore_case: bool,

    /// Turn on the page's -w
    #[arg(long, help_heading = WEB)]
    web_word_regexp: bool,

    /// Turn on the page's -x
    #[arg(long, help_heading = WEB)]
    web_line_regexp: bool,

    /// Turn on the page's -v
    #[arg(long, help_heading = WEB)]
    web_invert_match: bool,

    /// Set the page's -A
    #[arg(long, value_name = "N", help_heading = WEB)]
    web_after_context: Option<usize>,

    /// Set the page's -B
    #[arg(long, value_name = "N", help_heading = WEB)]
    web_before_context: Option<usize>,

    /// Set the page's -A and -B. --web-after-context and --web-before-context take precedence
    #[arg(long, value_name = "N", help_heading = WEB)]
    web_context: Option<usize>,
}

impl Args {
    /// The server-side filter, if a pattern was given.
    fn line_filter(&self) -> Option<Result<LineFilter, regex::Error>> {
        let patterns = match &self.pattern {
            Some(p) => std::slice::from_ref(p),
            None if self.patterns.is_empty() => return None,
            None => &self.patterns[..],
        };
        let opts = MatchOptions {
            fixed: self.fixed_strings,
            ignore_case: self.ignore_case,
            word: self.word_regexp,
            line: self.line_regexp,
        };
        Some(LineFilter::new(
            patterns,
            opts,
            self.invert_match,
            self.before_context.or(self.context).unwrap_or(0),
            self.after_context.or(self.context).unwrap_or(0),
        ))
    }

    /// The `--web-*` options as the page's URL query, or `None` if none were given.
    /// Uses the same parameters the page writes: grep's flag letters, plus `e` for the pattern.
    fn web_query(&self) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(pattern) = &self.web_regexp {
            parts.push(format!("e={}", url_encode(pattern)));
        }
        for (flag, on) in [
            ("i", self.web_ignore_case),
            ("v", self.web_invert_match),
            ("w", self.web_word_regexp),
            ("x", self.web_line_regexp),
            ("F", self.web_fixed_strings),
        ] {
            if on {
                parts.push(flag.to_string());
            }
        }
        if let Some(n) = self.web_after_context.or(self.web_context) {
            parts.push(format!("A={n}"));
        }
        if let Some(n) = self.web_before_context.or(self.web_context) {
            parts.push(format!("B={n}"));
        }
        (!parts.is_empty()).then(|| parts.join("&"))
    }
}

/// Percent-encodes everything except unreserved URL characters.
fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}


#[derive(Clone)]
enum Msg {
    Line(Arc<str>),
    Eof,
}

struct Shared {
    history: Vec<Arc<str>>,
    done: bool,
}

#[derive(Clone)]
struct AppState {
    shared: Arc<Mutex<Shared>>,
    tx: broadcast::Sender<Msg>,
    /// Query string from the `--web-*` options; a bare `/` redirects to it.
    web_query: Option<Arc<str>>,
}

impl AppState {
    /// Snapshot history and subscribe under the same lock, so no line is lost or duplicated.
    async fn subscribe(&self) -> (Vec<Arc<str>>, bool, broadcast::Receiver<Msg>) {
        let shared = self.shared.lock().await;
        (shared.history.clone(), shared.done, self.tx.subscribe())
    }

    /// Replay of history followed by live messages, ending after EOF.
    async fn messages(&self) -> impl Stream<Item = Msg> {
        let (history, done, rx) = self.subscribe().await;
        let replay = stream::iter(history.into_iter().map(Msg::Line));
        let live = if done {
            stream::iter(vec![Msg::Eof]).boxed()
        } else {
            BroadcastStream::new(rx)
                .filter_map(|m| async move { m.ok() })
                .boxed()
        };
        let mut finished = false;
        replay.chain(live).take_while(move |m| {
            let keep = !finished;
            finished = matches!(m, Msg::Eof);
            async move { keep }
        })
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    if std::io::stdin().is_terminal() {
        eprintln!("spiderpipe: nothing is piped in\n");
        Args::command().print_help().ok();
        std::process::exit(2);
    }
    let filter = match args.line_filter() {
        None => None,
        Some(Ok(f)) => Some(f),
        Some(Err(e)) => {
            eprintln!("spiderpipe: invalid pattern: {e}");
            std::process::exit(2);
        }
    };
    let (tx, _) = broadcast::channel(1024);
    let state = AppState {
        shared: Arc::new(Mutex::new(Shared {
            history: Vec::new(),
            done: false,
        })),
        tx,
        web_query: args.web_query().map(Into::into),
    };

    tokio::spawn(read_stdin(state.clone(), args.port, filter));

    let app = Router::new()
        .route("/", get(index))
        .route("/prism.js", get(prism))
        .route("/events", get(events))
        .route("/raw", get(raw))
        .with_state(state);

    let port = args.port;
    let host = if args.remote { "0.0.0.0" } else { "127.0.0.1" };
    let listener = match tokio::net::TcpListener::bind((host, port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("spiderpipe: failed to bind {host}:{port}: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("spiderpipe: streaming stdin at http://localhost:{port}");
    if args.remote {
        eprintln!(
            "spiderpipe: remote access enabled, reachable at http://<this-machine-ip>:{port}"
        );
    }
    // Handle Ctrl+C ourselves: SIGINT may arrive ignored (e.g. when started in the
    // background), and a graceful shutdown would hang on open live streams.
    tokio::select! {
        res = axum::serve(listener, app) => res.unwrap(),
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\nspiderpipe: stopped");
            std::process::exit(130);
        }
    }
}

async fn read_stdin(state: AppState, port: u16, mut filter: Option<LineFilter>) {
    let mut reader = BufReader::new(tokio::io::stdin());
    let mut buf = Vec::new();
    let mut total: u64 = 0;
    let mut error = None;
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) => break,
            Err(e) => {
                error = Some(e);
                break;
            }
            Ok(_) => {
                total += 1;
                let text = String::from_utf8_lossy(&buf);
                let line: Arc<str> = text.trim_end_matches(['\n', '\r']).into();
                let kept = match &mut filter {
                    Some(f) => f.push(line),
                    None => vec![line],
                };
                if kept.is_empty() {
                    continue;
                }
                let mut shared = state.shared.lock().await;
                for line in kept {
                    if shared.history.len() == HISTORY_LIMIT {
                        shared.history.remove(0);
                    }
                    shared.history.push(line.clone());
                    let _ = state.tx.send(Msg::Line(line));
                }
            }
        }
    }
    let mut shared = state.shared.lock().await;
    shared.done = true;
    let _ = state.tx.send(Msg::Eof);
    drop(shared);
    let plural = if total == 1 { "" } else { "s" };
    match error {
        Some(e) => eprintln!("spiderpipe: input failed after {total} line{plural}: {e}"),
        None => eprintln!("spiderpipe: input ended after {total} line{plural}"),
    }
    eprintln!("spiderpipe: still serving at http://localhost:{port} (Ctrl+C to quit)");
}

/// The web view. Without a query string it redirects to the `--web-*` defaults, if any.
async fn index(State(state): State<AppState>, RawQuery(query): RawQuery) -> Response {
    match (query, &state.web_query) {
        (None, Some(defaults)) => Redirect::temporary(&format!("/?{defaults}")).into_response(),
        _ => Html(include_str!("index.html")).into_response(),
    }
}

/// Prism syntax highlighter for the web view, fetched by build.rs and embedded so the page works offline.
async fn prism() -> impl IntoResponse {
    (
        [("content-type", "text/javascript; charset=utf-8")],
        include_str!(concat!(env!("OUT_DIR"), "/prism.js")),
    )
}

async fn events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = state.messages().await.map(|m| {
        Ok(match m {
            Msg::Line(line) => Event::default().data(&*line),
            Msg::Eof => Event::default().event("eof").data(""),
        })
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Plain-text stream, handy for `curl -N localhost:3000/raw`.
async fn raw(State(state): State<AppState>) -> impl IntoResponse {
    let stream = state.messages().await.filter_map(|m| async move {
        match m {
            Msg::Line(line) => Some(Ok::<_, Infallible>(Bytes::from(format!("{line}\n")))),
            Msg::Eof => None,
        }
    });
    (
        [("content-type", "text/plain; charset=utf-8")],
        Body::from_stream(stream),
    )
}
