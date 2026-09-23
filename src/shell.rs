use std::{path::PathBuf, sync::Arc};

use pathsearch::find_executable_in_path;
use rustyline::{Editor, history::DefaultHistory};

use crate::{
    command::builtin::{BuiltinFunc, Builtins},
    read::LineHelper,
};

pub struct ShellHandle {
    pub handle: Arc<tokio::sync::RwLock<Shell>>,
}

pub struct Shell {
    pub cwd: PathBuf,
    builtins: Builtins,
    editor: Editor<LineHelper, DefaultHistory>,
}

impl Shell {
    pub fn resolve_builtin(&self, builtin_name: &str) -> Option<BuiltinFunc> {
        self.builtins.get(builtin_name)
    }

    pub fn has_builtin(&self, builtin_name: &str) -> bool {
        self.resolve_builtin(builtin_name).is_some()
    }

    pub fn resolve_program(&self, program_name: &str) -> Option<PathBuf> {
        find_executable_in_path(program_name)
    }

    pub fn has_program(&self, program_name: &str) -> bool {
        self.resolve_program(program_name).is_some()
    }
}
