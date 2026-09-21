use std::path::PathBuf;

use rustyline::{Editor, history::DefaultHistory};

use crate::{command::builtin::Builtin, read::LineHelper};

pub struct Shell {
    pub cwd: PathBuf,
    builtins: Builtin<char>,
    editor: Editor<LineHelper, DefaultHistory>,
}

impl Shell {}
