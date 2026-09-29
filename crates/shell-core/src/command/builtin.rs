use std::{
    collections::HashMap, path::PathBuf, pin::Pin,
};

use crate::command::{
    Args, CommandError, CommandErrorKind, CommandResult, context::CommandContext,
};

pub type BuiltinFuture = Pin<Box<dyn Future<Output = Result<CommandResult, CommandError>> + Send>>;

pub type BuiltinFunc = fn(CommandContext, Args) -> BuiltinFuture;

pub struct Builtin {
    pub name: &'static str,
    function: BuiltinFunc,
}

#[derive(Debug)]
#[repr(transparent)]
pub struct Builtins(HashMap<&'static str, BuiltinFunc>);

impl Builtins {
    pub fn from_builtins(builtins: &[Builtin]) -> Self {
        let mut map = HashMap::new();

        for Builtin { name, function } in builtins {
            if map.insert(*name, *function).is_some() {
                panic!("tried to overwrite a builtin: {name}")
            }
        }

        Self(map)
    }

    pub fn get(&self, name: &str) -> Option<BuiltinFunc> {
        self.0.get(name).copied()
    }
}

macro_rules! builtin_list {
    ($($const_name:ident = $name:literal |mut $ctx:ident, $args:tt| $cmd:block);*) => {
        $(
            pub const $const_name: Builtin = Builtin {
                name: $name,
                #[allow(unused_mut)]
                function: |mut $ctx, $args| Box::pin(async move {
                    #[allow(unused_macros)]
                    macro_rules! btry {
                        ($expr:expr) => {
                            match $expr {
                                Ok(v) => v,
                                Err(err) => {
                                    use $crate::command::IntoCommandError;
                                    return Err(err.into_command_error(
                                        $name,
                                        $args.clone(),
                                    ));
                                }
                            }
                        };
                        ($expr:expr => $kind:ident()) => {
                            match $expr {
                                Ok(v) => v,
                                Err(err) => {
                                    return Err(CommandError::$kind(
                                        $name,
                                        $args.clone(),
                                        err,
                                    ));
                                }
                            }
                        }
                    }
                    #[allow(unused_macros)]
                    macro_rules! err {
                        ($err:expr) => {
                            use $crate::command::IntoCommandError;
                            return Err($err.into_command_error(
                                $name,
                                $args.clone(),
                            ));
                        };
                        ($err:expr => $kind:ident()) => {
                            return Err(CommandError::$kind(
                                $name,
                                $args.clone(),
                                $err,
                            ))
                        }
                    }
                    $cmd
                }),
            };
        )*

        pub const BUILTINS: &[Builtin] = &[$($const_name,)*];
    };
}

builtin_list!(
    EXIT = "exit" | mut _ctx, args | { match args.values.get(0) {
        Some(code_s) => match u8::from_str_radix(code_s, 10) {
            Ok(code) => Ok(CommandResult::Exit(code)),
            Err(e) => err!(e.to_string() => args()),
        },
        None => Ok(CommandResult::Exit(0)),
    } };

    ECHO = "echo" | mut ctx, args| {
        btry!(ctx.println(args.values.join(" ")).await);
        Ok(CommandResult::Continue)
    };

    TYPE = "type" |mut ctx, args| {
        let target = btry!(args
            .values
            .first()
            .ok_or("missing operand".to_string()) => args());

        if ctx.shell(|shell| shell.has_builtin(target)).await {
            btry!(ctx.println(format!("{} is a shell builtin", target))
                .await);
        } else if let Some(path) = ctx.shell(|shell| shell.resolve_program(target)).await {
            btry!(ctx.println(format!("{} is {}", target, path.display()))
                .await);
        } else {
            err!(target.clone() => not_found());
        }

        Ok(CommandResult::Continue)
    };

    PWD = "pwd" |mut ctx, args| {
        btry!(ctx.println(ctx.current_dir().await.display()).await);
        Ok(CommandResult::Continue)
    };

    CD = "cd" |mut ctx, args| {
        let new_path_str = match args.values.first() {
            Some(path) => path,
            None => return Ok(CommandResult::Continue),
        };

        let join_path = PathBuf::from(new_path_str);

        let new_path = if *new_path_str == "~" {
            btry!(std::env::home_dir()
                .ok_or(CommandErrorKind::Other("No HOME environemt variable set".to_string())))
        } else if join_path.is_absolute() {
            join_path
        } else {
            let components = join_path.components();

            let mut pathbuf = ctx.current_dir().await;

            for component in components {
                match component {
                    std::path::Component::CurDir => (),
                    std::path::Component::ParentDir => {
                        if !pathbuf.pop() {
                            err!("No such file or directory".to_string() => other());
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
            err!("No such file or directory".to_string() => other());
        }

        let _ = ctx.shell_mut(|mut shell| shell.cwd = new_path).await;

        Ok(CommandResult::Continue)
    }
);
