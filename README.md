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
brew tap elb3k/spiderpipe https://github.com/elb3k/spiderpipe
brew install spiderpipe
```

To build from the latest commit instead of a release, use `brew install --HEAD spiderpipe`.

### From source

You need [Rust](https://rustup.rs), plus `curl` and `tar` (both come with macOS, Linux and Windows 10+).

The build downloads [Prism](https://prismjs.com) v1.30.0, the syntax highlighter for the web page, from GitHub and checks it against a fixed SHA-256 checksum. Prism is not stored in this repository. To build without network access, download https://github.com/PrismJS/prism/archive/refs/tags/v1.30.0.tar.gz and set `SPIDERPIPE_PRISM_TARBALL` to its path.

```sh
make install                          # installs to ~/.local/bin
sudo make install PREFIX=/usr/local   # installs to /usr/local/bin
cargo install --path .                # installs to ~/.cargo/bin; also works on Windows
```

Run `make help` to see every target. To remove the binary, run `make uninstall`.

## Usage

```
Usage: spiderpipe [OPTIONS] [PATTERN]

Options:
  -p, --port <PORT>  Port to listen on [default: 3000]
  -r, --remote       Allow access from other machines (bind 0.0.0.0 instead of 127.0.0.1)
  -h, --help         Print help
  -V, --version      Print version

Filter (drops lines before they are stored, like grep):
  -e, --regexp <PATTERN>    Use PATTERN for matching. Repeat to keep lines that match any of them
  -E, --extended-regexp     Accepted for grep compatibility. Patterns are always extended regular expressions
  -F, --fixed-strings       Treat patterns as plain text, not regular expressions
  -i, --ignore-case         Ignore case when matching
  -w, --word-regexp         Match only whole words
  -x, --line-regexp         Match only whole lines
  -v, --invert-match        Keep the lines that do not match instead
  -A, --after-context <N>   Also keep N lines after each match
  -B, --before-context <N>  Also keep N lines before each match
  -C, --context <N>         Also keep N lines before and after each match. -A and -B take precedence
  [PATTERN]                 Keep only lines that match PATTERN, as in `grep PATTERN`

Web page defaults (opening / redirects to a URL with these set; each viewer can change them):
      --web-regexp <PATTERN>    Pattern to put in the page's filter box
      --web-fixed-strings       Turn on the page's -F
      --web-ignore-case         Turn on the page's -i
      --web-word-regexp         Turn on the page's -w
      --web-line-regexp         Turn on the page's -x
      --web-invert-match        Turn on the page's -v
      --web-after-context <N>   Set the page's -A
      --web-before-context <N>  Set the page's -B
      --web-context <N>         Set the page's -A and -B. --web-after-context and --web-before-context take precedence
```

Examples:

```sh
ping example.com | spiderpipe -p 8080                    # use another port
make 2>&1 | spiderpipe --remote                          # let other machines on the network watch
tail -f app.log | spiderpipe ERROR -C 3                  # keep only errors, with 3 lines around each
tail -f app.log | spiderpipe -i -e error -e panic        # keep lines with "error" or "panic", any case
tail -f app.log | spiderpipe --web-regexp warn --web-ignore-case
                                                         # keep everything, open the page filtered to "warn"
```

To include error output, redirect stderr into the pipe with `2>&1`.

### Filtering, the same as grep

There are two filters, and both work like `grep`:

- **On the command line**, `PATTERN` or `-e PATTERN` and the options with it filter on the server. spiderpipe drops lines that don't match before it stores them, so no viewer sees them, not even on `/raw`.
- **On the page**, the filter box and its buttons filter in the browser. Every line is still stored, and each viewer has their own filter. The `--web-*` options only set the page's starting values; a viewer can change them. See [Page URL](#page-url).

You can use both at once. The page then filters the lines that the command-line filter kept.

Both filters use grep's flags with grep's meaning:

| grep            | Command line            | Page            | Page default from the command line |
|-----------------|-------------------------|-----------------|------------------------------------|
| `PATTERN`       | `PATTERN` or `-e PATTERN` (repeatable) | filter box | `--web-regexp PATTERN` |
| `-i`            | `-i`, `--ignore-case`   | `-i` button     | `--web-ignore-case`                |
| `-v`            | `-v`, `--invert-match`  | `-v` button     | `--web-invert-match`               |
| `-w`            | `-w`, `--word-regexp`   | `-w` button     | `--web-word-regexp`                |
| `-x`            | `-x`, `--line-regexp`   | `-x` button     | `--web-line-regexp`                |
| `-F`            | `-F`, `--fixed-strings` | `-F` button     | `--web-fixed-strings`              |
| `-E`            | `-E` (always on)        | always on       |                                    |
| `-A N`          | `-A N`, `--after-context N`  | `-A` field | `--web-after-context N`          |
| `-B N`          | `-B N`, `--before-context N` | `-B` field | `--web-before-context N`         |
| `-C N`          | `-C N`, `--context N`   | set `-A` and `-B` | `--web-context N`                |

The page's URL uses the same letters. See [Page URL](#page-url).

As in grep, a pattern is a regular expression and matching is case-sensitive unless you use `-i`. Context lines come with `--` between groups that are not next to each other. On the page, context lines are dimmed. Both filters give the same output as GNU grep 3.11 for these flags.

Where they differ from grep:

- **Regular expressions are always extended**, as with `grep -E`. grep's default basic syntax is not supported, so `a|b` means "a or b" without `-E`. The command line uses [Rust's regex syntax](https://docs.rs/regex/latest/regex/#syntax), which has no backreferences or lookaround. The page uses JavaScript regular expressions.
- **`-A` and `-B` take precedence over `-C`**, whatever their order, as in GNU grep. BSD grep, the default on macOS, uses the one given last.
- **The page takes one pattern.** For several, use `a|b`.
- **An empty pattern on the page shows every line**, even with `-v`.
- **Other grep flags**, such as `-c`, `-m`, `-n` and `-o`, are not supported.

### Page URL

The page keeps its filter and syntax in the URL and updates it as you change them, so you can bookmark or share a view. The parameters use grep's flag letters:

| Parameter     | Meaning                                                  |
|---------------|----------------------------------------------------------|
| `e=PATTERN`   | Filter pattern (URL-encoded)                             |
| `i`, `v`, `w`, `x`, `F` | grep options that are on. Leave one out to turn it off |
| `A=N`, `B=N`  | Lines after and before each match                        |
| `C=N`         | Sets both `A` and `B`. `A` and `B` take precedence       |
| `syntax=LANG` | Syntax highlighting: `log`, `json`, `diff`, `yaml`, `toml`, `bash`, `javascript`, `python`, `sql`, `docker` or `markup` |

For example, `http://localhost:3000/?e=error|panic&i&C=2&syntax=log` shows lines with "error" or "panic" in any case, with 2 lines around each, highlighted as a log.

When you start spiderpipe with `--web-*` options, opening `/` with no parameters redirects to a URL with those options set. A URL with any parameters is used as it is. When you clear every control, the page writes the URL as `/?e=`, so a reload does not bring the defaults back.

Without `syntax` in the URL, the page uses the syntax you chose last in this browser.

### Endpoints

| Path      | What it serves                                          |
|-----------|---------------------------------------------------------|
| `/`       | Live web view. Takes the [page URL](#page-url) parameters |
| `/raw`    | Plain-text stream: `curl -N http://localhost:3000/raw`  |
| `/events` | Server-sent events, one event per line                  |

## Behavior

- **History:** spiderpipe keeps the last 10,000 lines. A viewer who opens the page late sees recent output first, then new lines.
- **End of input:** when the piped command finishes, spiderpipe prints `input ended after N lines` to stderr. The page shows `── input ended ──`. The server keeps running, so you can still open the page or save the output with `curl http://localhost:3000/raw > out.txt`.
- **Stopping:** press Ctrl+C. In a terminal, Ctrl+C also stops the command that feeds the pipe.
- **Filter:** the box in the page header filters lines like grep. See [Filtering, the same as grep](#filtering-the-same-as-grep). Press `/` to focus the box and Esc to clear it.
- **Syntax:** the dropdown highlights lines with [Prism](https://prismjs.com): Log, JSON, Diff, YAML, TOML, Shell, JavaScript, Python, SQL, Dockerfile, or HTML/XML. The choice goes in the [page URL](#page-url), and the browser also remembers it. Each line is highlighted on its own, so something that spans lines, such as a block comment, is not colored as one piece. Prism is built into the binary, so the page works offline. To add a language, add it to `PRISM_LANGS` in [build.rs](build.rs) and add an `<option>` for it in `src/index.html`.
- **Terminal colors:** the web page removes terminal color codes.
- **Output:** spiderpipe writes its own messages to stderr only. It does not copy the piped input to its own output.

## Security

By default, only this machine can connect. With `--remote`, anyone on your network who can reach the port can see the stream. There is no password, so do not use `--remote` with secrets or on untrusted networks.

## Releasing

```sh
make tag-release VERSION=0.2.0
```

This bumps `version` in `Cargo.toml`, commits, tags `v0.2.0`, and pushes `main` and the tag. The push starts the [formula workflow](.github/workflows/formula.yml), which downloads the tag's source tarball, writes its `url` and `sha256` into `Formula/spiderpipe.rb`, and commits that to `main`. After about a minute, run `git pull` to get that commit. Then `brew upgrade spiderpipe` installs the new version.

The formula cannot hold the checksum of a tarball that contains the formula itself. So the formula update is always a separate commit after the tag. Never move or re-push a tag that was already released: that changes the tarball and breaks the checksum.
