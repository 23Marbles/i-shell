use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::Arc,
};

use pathsearch::find_executable_in_path;
use rustyline::{Editor, error::ReadlineError, history::DefaultHistory};
use tokio::sync::RwLock;

use crate::{
    command::{
        CommandContextBuilder, CommandError, CommandResult,
        builtin::{BUILTINS, BuiltinFunc, Builtins},
        program::Program,
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

    pub fn interactive() -> InteractiveShell {
        InteractiveShell {
            shell_handle: ShellHandle {
                handle: Arc::new(RwLock::new(Self::default())),
            },
            editor: Editor::new().unwrap(),
        }
    }
}

pub struct InteractiveShell {
    shell_handle: ShellHandle,
    editor: Editor<LineHelper, DefaultHistory>,
}

impl InteractiveShell {
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
            let line = match self.editor.readline(&format!(
                "{}>> ",
                self.shell(|shell| {
                    #[cfg(target_os = "windows")]
                    return shell.cwd.clone().into_os_string();

                    #[cfg(not(target_os = "windows"))]
                    return shell
                        .cwd
                        .to_path_buf()
                        .file_name()
                        .map_or(std::ffi::OsString::new(), |n| n.to_os_string());
                })
                .await
                .display()
            )) {
                Ok(s) => {
                    self.editor.add_history_entry(&s)?;
                    s
                }
                Err(ReadlineError::Eof) => return Ok(()),
                Err(ReadlineError::Interrupted) => {
                    println!("\x1b[31m^C\x1b[0m");
                    continue;
                }
                Err(e) => return Err(e),
            };

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
        } else if let Some(path) = self.shell(|this| this.resolve_program(&name)).await {
            let program = Program::run(path, args, ctx).await?;

            program.monitor().await?;

            Ok(CommandResult::Continue)
        } else {
            Err(CommandError::not_found(name.clone(), args, name))
        }
    }
}
