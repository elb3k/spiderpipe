# spiderpipe

Pipe any command's output into `spiderpipe` and watch it live in a browser.

```sh
tail -f app.log | spiderpipe
# spiderpipe: streaming stdin at http://localhost:3000
```

Open http://localhost:3000 and new lines appear as the command writes them.

## Install

### Homebrew

```sh
brew tap OWNER/spiderpipe https://github.com/OWNER/spiderpipe
brew install spiderpipe
```

To build from the latest commit instead of a release, use `brew install --HEAD spiderpipe`.

### From source

You need [Rust](https://rustup.rs).

```sh
make install                          # installs to ~/.local/bin
sudo make install PREFIX=/usr/local   # installs to /usr/local/bin
cargo install --path .                # installs to ~/.cargo/bin; also works on Windows
```

Run `make help` to see every target. To remove the binary, run `make uninstall`.

## Usage

```
spiderpipe [OPTIONS]

  -p, --port <PORT>  Port to listen on [default: 3000]
  -r, --remote       Allow access from other machines (bind 0.0.0.0 instead of 127.0.0.1)
  -h, --help         Print help
  -V, --version      Print version
```

Examples:

```sh
ping example.com | spiderpipe -p 8080   # use another port
make 2>&1 | spiderpipe --remote         # let other machines on the network watch
```

To include error output, redirect stderr into the pipe with `2>&1`.

### Endpoints

| Path      | What it serves                                          |
|-----------|---------------------------------------------------------|
| `/`       | Live web view                                           |
| `/raw`    | Plain-text stream: `curl -N http://localhost:3000/raw`  |
| `/events` | Server-sent events, one event per line                  |

## Behavior

- **History:** spiderpipe keeps the last 10,000 lines. A viewer who opens the page late sees recent output first, then new lines.
- **End of input:** when the piped command finishes, spiderpipe prints `input ended after N lines` to stderr. The page shows `── input ended ──`. The server keeps running, so you can still open the page or save the output with `curl http://localhost:3000/raw > out.txt`.
- **Stopping:** press Ctrl+C. In a terminal, Ctrl+C also stops the command that feeds the pipe.
- **Terminal colors:** the web page removes terminal color codes.
- **Output:** spiderpipe writes its own messages to stderr only. It does not copy the piped input to its own output.

## Security

By default, only this machine can connect. With `--remote`, anyone on your network who can reach the port can see the stream. There is no password, so do not use `--remote` with secrets or on untrusted networks.

## Releasing

1. Update `version` in `Cargo.toml` and commit.
2. Tag the release and push the tag:
   ```sh
   git tag v0.1.0 && git push origin v0.1.0
   ```
3. Compute the checksum of the source tarball that GitHub creates:
   ```sh
   curl -sL https://github.com/OWNER/spiderpipe/archive/refs/tags/v0.1.0.tar.gz | shasum -a 256
   ```
4. In `Formula/spiderpipe.rb`, update `url` and `sha256`, then commit and push.
5. Check the formula:
   ```sh
   brew install --build-from-source spiderpipe && brew test spiderpipe
   ```
