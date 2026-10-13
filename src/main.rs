use std::{convert::Infallible, io::IsTerminal, sync::Arc};

use axum::{
    body::{Body, Bytes},
    extract::State,
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse,
    },
    routing::get,
    Router,
};
use clap::{CommandFactory, Parser};
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

Endpoints:
  /         live web view
  /raw      plain-text stream (curl -N http://localhost:3000/raw)
  /events   server-sent events, one event per line

The last 10000 lines are kept, so late viewers see recent output.
After the input ends the server keeps running until Ctrl+C.";

/// Stream whatever is piped into stdin to a web page.
#[derive(Parser)]
#[command(version, after_help = AFTER_HELP)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value_t = 3000)]
    port: u16,

    /// Allow access from other machines (bind 0.0.0.0 instead of 127.0.0.1)
    #[arg(short, long)]
    remote: bool,
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
    let (tx, _) = broadcast::channel(1024);
    let state = AppState {
        shared: Arc::new(Mutex::new(Shared {
            history: Vec::new(),
            done: false,
        })),
        tx,
    };

    tokio::spawn(read_stdin(state.clone(), args.port));

    let app = Router::new()
        .route("/", get(index))
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
        eprintln!("spiderpipe: remote access enabled, reachable at http://<this-machine-ip>:{port}");
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

async fn read_stdin(state: AppState, port: u16) {
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
                let mut shared = state.shared.lock().await;
                if shared.history.len() == HISTORY_LIMIT {
                    shared.history.remove(0);
                }
                shared.history.push(line.clone());
                let _ = state.tx.send(Msg::Line(line));
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

async fn index() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

async fn events(State(state): State<AppState>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
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
    ([("content-type", "text/plain; charset=utf-8")], Body::from_stream(stream))
}
