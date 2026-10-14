//! Server-side line filter with grep-style context lines (`-A`, `-B`, `-C`).

use std::{collections::VecDeque, sync::Arc};

use regex::{Regex, RegexBuilder};

/// Printed between groups of lines that are not next to each other, as grep does.
pub const SEPARATOR: &str = "--";

/// How patterns match, like grep's `-F`, `-i`, `-w` and `-x`.
#[derive(Clone, Copy, Default)]
pub struct MatchOptions {
    /// Patterns are plain text, not regular expressions (`-F`).
    pub fixed: bool,
    /// Ignore case (`-i`).
    pub ignore_case: bool,
    /// A match must be a whole word (`-w`).
    pub word: bool,
    /// A match must be the whole line (`-x`); overrides `word`.
    pub line: bool,
}

pub struct LineFilter {
    re: Regex,
    invert: bool,
    before: usize,
    after: usize,
    /// Recent dropped lines, kept in case a later match needs them as before-context.
    held: VecDeque<Arc<str>>,
    /// How many more lines to keep as after-context of the last match.
    after_left: usize,
    /// Whether any line has been kept yet.
    kept_any: bool,
    /// Whether a line was dropped since the last kept line.
    gap: bool,
}

impl LineFilter {
    /// Selects lines that match any of `patterns` (or that match none, if `invert`),
    /// plus `before` and `after` lines of context.
    pub fn new(
        patterns: &[String],
        opts: MatchOptions,
        invert: bool,
        before: usize,
        after: usize,
    ) -> Result<Self, regex::Error> {
        let alternatives: Vec<String> = patterns
            .iter()
            .map(|p| format!("(?:{})", if opts.fixed { regex::escape(p) } else { p.clone() }))
            .collect();
        let any = alternatives.join("|");
        let pattern = if opts.line {
            format!("^(?:{any})$")
        } else if opts.word {
            format!(r"\b(?:{any})\b")
        } else {
            any
        };
        let re = RegexBuilder::new(&pattern)
            .case_insensitive(opts.ignore_case)
            .build()?;
        Ok(Self {
            re,
            invert,
            before,
            after,
            held: VecDeque::with_capacity(before),
            after_left: 0,
            kept_any: false,
            gap: false,
        })
    }

    /// Feeds one input line and returns the lines to keep, including any context and separator.
    pub fn push(&mut self, line: Arc<str>) -> Vec<Arc<str>> {
        if self.re.is_match(&line) != self.invert {
            let mut out = Vec::with_capacity(self.held.len() + 2);
            if self.kept_any && self.gap && (self.before > 0 || self.after > 0) {
                out.push(SEPARATOR.into());
            }
            out.extend(self.held.drain(..));
            out.push(line);
            self.after_left = self.after;
            self.kept_any = true;
            self.gap = false;
            out
        } else if self.after_left > 0 {
            self.after_left -= 1;
            vec![line]
        } else {
            if self.before == 0 {
                self.gap = true;
            } else {
                if self.held.len() == self.before {
                    self.held.pop_front();
                    self.gap = true;
                }
                self.held.push_back(line);
            }
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `l1` to `l20`, with "hit" on lines 5, 7 and 15.
    fn input() -> Vec<String> {
        (1..=20)
            .map(|i| match i {
                5 | 7 | 15 => format!("l{i} hit"),
                _ => format!("l{i}"),
            })
            .collect()
    }

    fn run(filter: LineFilter) -> String {
        let mut filter = filter;
        input()
            .into_iter()
            .flat_map(|l| filter.push(l.into()))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn grep(before: usize, after: usize) -> String {
        run(LineFilter::new(&["hit".into()], MatchOptions::default(), false, before, after).unwrap())
    }

    // Expected output comes from running the same input through `grep`.
    #[test]
    fn context_matches_grep() {
        assert_eq!(grep(0, 0), "l5 hit l7 hit l15 hit");
        assert_eq!(grep(1, 1), "l4 l5 hit l6 l7 hit l8 -- l14 l15 hit l16");
        assert_eq!(
            grep(3, 0),
            "l2 l3 l4 l5 hit l6 l7 hit -- l12 l13 l14 l15 hit"
        );
        assert_eq!(grep(0, 2), "l5 hit l6 l7 hit l8 l9 -- l15 hit l16 l17");
        assert_eq!(grep(10, 10), input().join(" "));
    }

    #[test]
    fn options() {
        let f = |p: &[&str], opts, invert| {
            let p: Vec<String> = p.iter().map(|s| s.to_string()).collect();
            LineFilter::new(&p, opts, invert, 0, 0).unwrap()
        };
        let none = MatchOptions::default();
        let icase = MatchOptions { ignore_case: true, ..none };
        let fixed = MatchOptions { fixed: true, ..none };
        let word = MatchOptions { word: true, ..none };
        let line = MatchOptions { line: true, ..none };
        assert_eq!(run(f(&["HIT"], none, false)), "");
        assert_eq!(run(f(&["HIT"], icase, false)), "l5 hit l7 hit l15 hit");
        assert_eq!(run(f(&["^l1[0-3]$", "l7"], none, false)), "l7 hit l10 l11 l12 l13");
        assert_eq!(run(f(&["l1."], fixed, false)), "");
        assert_eq!(run(f(&["l1"], word, false)), "l1");
        assert_eq!(run(f(&["l1."], line, false)), "l10 l11 l12 l13 l14 l16 l17 l18 l19");
        assert_eq!(run(f(&["[0-9]"], none, true)), "");
        assert_eq!(run(f(&["hit|l2"], none, true)).split(' ').count(), 20 - 3 - 2);
    }
}
