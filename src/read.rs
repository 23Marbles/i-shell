use rustyline::{
    Helper,
    completion::{self, Completer},
    highlight::Highlighter,
    hint::Hinter,
    validate::Validator,
};

pub struct Args {
    pub values: Vec<String>,
}

pub struct LineHelper {}

impl Hinter for LineHelper {
    type Hint = String;
}
impl Completer for LineHelper {
    type Candidate = completion::Pair;
}
impl Highlighter for LineHelper {}
impl Validator for LineHelper {}
impl Helper for LineHelper {}
