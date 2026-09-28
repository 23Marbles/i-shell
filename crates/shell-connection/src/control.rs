use serde::{Deserialize, Serialize};
use shell_core::command::{Command, wire::WireCommandError};
use std::{io, marker::PhantomData};
use tokio::net::TcpStream;

use crate::{read_msg, write_msg};

#[async_trait::async_trait]
pub trait ControlKind {}

pub struct Controller;
impl ControlKind for Controller {}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandId(u32);

#[derive(Debug, Serialize, Deserialize)]
pub enum ControlMessage {
    RunCommand { cmd: Command, id: CommandId },
}

pub struct Remote;
impl ControlKind for Remote {}

#[derive(Debug, Serialize, Deserialize)]
pub enum ResponseMessage {
    CommandError {
        err: WireCommandError,
        cmd_id: CommandId,
    },
}

pub struct ControlConnection<K: ControlKind> {
    pub stream: TcpStream,
    _kind: PhantomData<K>,
}

impl<K: ControlKind> From<TcpStream> for ControlConnection<K> {
    fn from(value: TcpStream) -> Self {
        Self {
            stream: value,
            _kind: PhantomData,
        }
    }
}

impl ControlConnection<Controller> {
    pub async fn send_cmd(&mut self, cmd: impl Into<ControlMessage>) -> Result<(), io::Error> {
        write_msg(&mut self.stream, &cmd.into()).await
    }
}

impl ControlConnection<Remote> {
    pub async fn recv_cmd(&mut self) -> Result<ControlMessage, io::Error> {
        read_msg(&mut self.stream).await
    }
}

impl ControlConnection<Remote> {
    pub async fn send_response(
        &mut self,
        cmd: impl Into<ResponseMessage>,
    ) -> Result<(), io::Error> {
        write_msg(&mut self.stream, &cmd.into()).await
    }
}

impl ControlConnection<Controller> {
    pub async fn recv_response(&mut self) -> Result<ResponseMessage, io::Error> {
        read_msg(&mut self.stream).await
    }
}

pub struct Negotiating;
impl ControlKind for Negotiating {}

#[derive(Debug, thiserror::Error)]
pub enum NegotiationError {
    #[error(transparent)]
    IoErr(#[from] std::io::Error),

    #[error("expected an acceptance message")]
    ExpectedAccept,

    #[error("expected an attempt message")]
    ExpectedAttempt,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum NegotiationMessage {
    AttemptConnect,
    AcceptConnection,
}

impl ControlConnection<Negotiating> {
    pub async fn attempt_conn(&mut self) -> Result<(), NegotiationError> {
        write_msg(&mut self.stream, &NegotiationMessage::AttemptConnect).await?;

        if read_msg::<NegotiationMessage>(&mut self.stream).await?
            != NegotiationMessage::AttemptConnect
        {
            return Err(NegotiationError::ExpectedAttempt);
        }

        write_msg(&mut self.stream, &NegotiationMessage::AcceptConnection).await?;

        if read_msg::<NegotiationMessage>(&mut self.stream).await?
            != NegotiationMessage::AcceptConnection
        {
            return Err(NegotiationError::ExpectedAttempt);
        }

        Ok(())
    }
}
