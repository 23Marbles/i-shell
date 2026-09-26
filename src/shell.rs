use std::{ops::{Deref, DerefMut}, path::PathBuf, str::FromStr, sync::Arc};

use pathsearch::find_executable_in_path;
use rustyline::{Editor, error::ReadlineError, history::DefaultHistory};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{
    command::{
        CommandContextBuilder, CommandError, CommandResult,
        builtin::{BUILTINS, BuiltinFunc, Builtins},
    },
    parse::{CommandSpec, Pipeline},
    read::LineHelper,
};

#[derive(Debug, Clone)]
pub struct ShellHandle {
    pub handle: Arc<tokio::sync::RwLock<Shell>>,
}

impl From<Shell> for ShellHandle {
    fn from(value: Shell) -> Self {
        Self {
            handle: Arc::new(RwLock::new(value)),
        }
    }
}

#[derive(Debug)]
pub struct Shell {
    pub cwd: PathBuf,
    builtins: Builtins,
    editor: Editor<LineHelper, DefaultHistory>,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or(if let Some(path) = std::env::home_dir() {
                std::env::set_current_dir(&path).unwrap();
                path
            } else {
                panic!()
            }),
            builtins: Builtins::from_builtins(BUILTINS),
            editor: Editor::new().unwrap(),
        }
    }
}

impl Shell {
    #[inline]
    pub fn resolve_builtin(&self, builtin_name: &str) -> Option<BuiltinFunc> {
        self.builtins.get(builtin_name)
    }

    #[inline]
    pub fn has_builtin(&self, builtin_name: &str) -> bool {
        self.resolve_builtin(builtin_name).is_some()
    }

    #[inline]
    pub fn resolve_program(&self, program_name: &str) -> Option<PathBuf> {
        find_executable_in_path(program_name)
    }

    #[inline]
    pub fn has_program(&self, program_name: &str) -> bool {
        self.resolve_program(program_name).is_some()
    }

    pub fn new() -> ShellRunner {
        ShellRunner {
            shell_handle: ShellHandle {
                handle: Arc::new(RwLock::new(Self::default())),
            },
        }
    }
}

pub struct ShellRunner {
    shell_handle: ShellHandle,
}

impl ShellRunner {
    #[must_use]
    pub async fn shell<R>(&self, func: impl FnOnce(&Shell) -> R) -> R {
        (func)(self.shell_handle.handle.read().await.deref())
    }

    #[must_use]
    pub async fn shell_mut<R>(&self, func: impl FnOnce(&mut Shell) -> R) -> R {
        (func)(self.shell_handle.handle.write().await.deref_mut())
    }

    pub async fn run(mut self) -> Result<(), ReadlineError> {
        loop {
            let line = self.shell_mut(|this| this.editor.readline(">> ")).await?;

            let pipeline = Pipeline::try_from(line).unwrap();

            match self.run_pipeline(pipeline).await {
                Ok(CommandResult::Continue) => (),
                Ok(CommandResult::Exit(code)) => std::process::exit(code),
                Err(e) => eprintln!("{e}"),
            };
        }
    }

    pub async fn run_pipeline(
        &mut self,
        pipeline: Pipeline,
    ) -> Result<CommandResult, CommandError> {
        let CommandSpec { name, args } = pipeline.cmd;

        let ctx = CommandContextBuilder::new(&self.shell_handle).build();

        if let Some(func) = self.shell(|this| this.resolve_builtin(&name)).await {
            (func)(ctx, args).await
        } else {
            return Err(CommandError::not_found(name.clone(), args, name));
        }
    }
}
