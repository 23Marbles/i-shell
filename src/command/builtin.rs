use std::collections::HashMap;

use crate::{
    command::{CommandContext, CommandError, CommandResult},
    read::Args,
    streams::{StreamRx, StreamTx},
};

pub trait TypedBuiltin: Send + Sync + 'static {
    type Input: Send + 'static;
    type Output: Send + 'static;

    fn run(
        self,
        stdin: StreamRx<Self::Input>,
        stdout: StreamTx<Self::Output>,
        args: Args,
    ) -> impl Future<Output = Result<CommandResult, CommandError>> + Send;
}

pub type BuiltinFunc<T> = fn(CommandContext<T>, &Args) -> Result<CommandResult, CommandError>;

pub struct Builtin<T> {
    pub name: &'static str,
    function: BuiltinFunc<T>,
}

#[repr(transparent)]
pub struct Builtins<T>(HashMap<&'static str, BuiltinFunc<T>>);

impl<T> Builtins<T> {
    pub fn from_builtins(builtins: &[Builtin<T>]) -> Self {
        let mut map = HashMap::new();

        for Builtin { name, function } in builtins {
            if map.insert(*name, *function).is_some() {
                panic!("tried to overwrite a builtin: {name}")
            }
        }

        Self(map)
    }

    pub fn get(&self, name: &str) -> Option<BuiltinFunc<T>> {
        self.0.get(name).map(|f| *f)
    }
}

pub const EXIT: Builtin = Builtin {
    name: "exit",
    // TODO:
    // Exiting with exit codes using args
    function: |_, _| std::process::exit(0),
};

pub const ECHO: Builtin = Builtin {
    name: "echo",
    function: |_, args, w, _, _| {
        writeln!(w, "{}", args.join(" ")).unwrap();
        Ok(())
    },
};

pub const TYPE: Builtin = Builtin {
    name: "type",
    function: |shell, args, w, _, _| {
        let target = args.get(0).ok_or("type: missing operand".to_string())?;

        if shell.has_builtin(&target) {
            writeln!(w, "{} is a shell builtin", target).unwrap();
        } else if let Some(path) = shell.resolve_program(target) {
            writeln!(w, "{} is {}", args[0], path.display()).unwrap();
        } else {
            return Err(format!("{}: not found", target));
        }

        Ok(())
    },
};

pub const PWD: Builtin = Builtin {
    name: "pwd",
    function: |shell, _, w, _, _| {
        writeln!(w, "{}", shell.cwd.to_string_lossy()).unwrap();
        Ok(())
    },
};

pub const CD: Builtin = Builtin {
    name: "cd",
    function: |shell, args, _, _, _| {
        let new_path_str = match args.first() {
            Some(path) => path,
            None => return Ok(()),
        };

        let join_path = PathBuf::from(*new_path_str);

        let new_path = if *new_path_str == "~" {
            let home_dir = std::env::var_os("HOME")
                .ok_or(format!("cd: ~: No HOME environemt variable set"))?;

            PathBuf::from(home_dir)
        } else if join_path.is_absolute() {
            join_path
        } else {
            let components = join_path.components();

            let mut pathbuf = shell.cwd.clone();

            for component in components {
                match component {
                    std::path::Component::CurDir => (),
                    std::path::Component::ParentDir => {
                        if !pathbuf.pop() {
                            return Err(format!("cd: {new_path_str}: No such file or directory"));
                        }
                    }
                    std::path::Component::Normal(os_str) => pathbuf.push(os_str),

                    std::path::Component::Prefix(_) => todo!(),
                    std::path::Component::RootDir => todo!(),
                }
            }

            pathbuf
        };

        if !new_path.exists() {
            return Err(format!("cd: {new_path_str}: No such file or directory"));
        }

        shell.cwd = new_path;

        Ok(())
    },
};

pub const BUILTINS: &[Builtin] = &[EXIT, ECHO, TYPE, PWD, CD];
