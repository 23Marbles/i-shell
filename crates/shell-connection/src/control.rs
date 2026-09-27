use serde::{Deserialize, Serialize};
use shell_core::command::{Args, Command};
use std::{io, marker::PhantomData};
use tokio::{net::TcpStream};

use crate::{read_msg, write_msg};

#[async_trait::async_trait]
pub trait ControlKind {}

pub struct Controller;

#[async_trait::async_trait]
impl ControlKind for Controller {}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransferCommand {
    name: String,
    args: Vec<String>,
}

impl From<TransferCommand> for Command {
    fn from(value: TransferCommand) -> Self {
        let TransferCommand { name, args } = value;
        Self { name, args: Args { values: args } }
    }
}

impl From<Command> for TransferCommand {
    fn from(value: Command) -> Self {
        let Command { name, args: Args { values: args } } = value;
        Self { name, args }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ControlCommand {
    RunCommand { cmd: TransferCommand },
}

pub struct Remote;
impl ControlKind for Remote {}

pub struct ControlConnection<K: ControlKind> {
    stream: TcpStream,
    _kind: PhantomData<K>,
}

impl ControlConnection<Controller> {
    pub async fn send_cmd(&mut self, cmd: &ControlCommand) -> Result<(), io::Error> {
        write_msg(&mut self.stream, cmd).await
    }
}

impl ControlConnection<Remote> {
    pub async fn recv_cmd(&mut self) -> Result<ControlCommand, io::Error> {
        read_msg(&mut self.stream).await
    }
}