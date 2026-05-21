use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::{Validator, ValidationResult, ValidationContext};
use rustyline::{Context, Helper, Result as RustyResult};
use std::borrow::Cow;

const COMMANDS: &[&str] = &[
    "help",
    "pwd", "lpwd", "quota",
    "ls", "lls",
    "cd", "lcd",
    "mkdir", "lmkdir",
    "search", "semsearch",
    "put", "get", "mget",
    "rename", "mv", "cp", "rm",
    "lmv", "lcp", "lrm",
    "clear",
    "exit", "quit", "bye",
];

#[derive(Debug)]
pub struct BftpHelper;

impl Completer for BftpHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> RustyResult<(usize, Vec<Pair>)> {
        let line_prefix = &line[..pos];
        let (start, word) = match line_prefix.rsplit_once(char::is_whitespace) {
            Some((_, w)) => (line_prefix.len() - w.len(), w),
            None => (0, line_prefix),
        };
        let matches: Vec<Pair> = COMMANDS
            .iter()
            .filter(|cmd| cmd.starts_with(word))
            .map(|cmd| Pair {
                display: cmd.to_string(),
                replacement: cmd.to_string(),
            })
            .collect();
        Ok((start, matches))
    }
}

impl Hinter for BftpHelper {
    type Hint = String;
    fn hint(&self, _line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<String> {
        None
    }
}

impl Highlighter for BftpHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Borrowed(line)
    }
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, _default: bool) -> Cow<'b, str> {
        Cow::Borrowed(prompt)
    }
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Borrowed(hint)
    }
}

impl Validator for BftpHelper {
    fn validate(&self, _ctx: &mut ValidationContext) -> RustyResult<ValidationResult> {
        Ok(ValidationResult::Valid(None))
    }
    fn validate_while_typing(&self) -> bool {
        false
    }
}

impl Helper for BftpHelper {}
