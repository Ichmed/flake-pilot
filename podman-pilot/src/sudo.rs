use std::{
    ffi::OsStr,
    ops::DerefMut,
    process::{Command, ExitStatus, Output, Stdio},
};

use crate::defaults::debug;

pub struct Sudo(Command);

impl Sudo {
    pub fn new() -> Self {
        Self(Command::new("sudo"))
    }

    pub fn void(&mut self) -> &mut Self {
        self.stderr(Stdio::null()).stdout(Stdio::null());
        self
    }

    pub fn user(&mut self, user: Option<&str>) -> &mut Self {
        if let Some(user) = user {
            self.0.arg("--user").arg(user);
        }
        self
    }

    pub fn flag(&mut self, condition: bool, arg: &str) -> &mut Self {
        if condition {
            self.0.arg(arg);
        }
        self
    }

    pub fn arg<S>(&mut self, arg: S) -> &mut Self
    where
        S: AsRef<OsStr>,
    {
        self.0.arg(arg);
        self
    }

    pub fn debug_print(&mut self) -> &mut Self {
        debug(&format!("{:?}", self.0.get_args()));
        self
    }
}

impl std::ops::Deref for Sudo {
    type Target = Command;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Sudo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for Sudo {
    fn default() -> Self {
        Self::new()
    }
}

pub trait OutputExtension {
    fn lossy_stdout(&self) -> std::borrow::Cow<'_, str>;
    fn lossy_stderr(&self) -> std::borrow::Cow<'_, str>;

    /// Return Ok(()) on success or a ProcessError::Fail with the underlying output and Errorcode
    fn report(self) -> Result<Output, ProcessError>;

    fn debug_print(&self) {
        debug(&self.lossy_stdout());
        debug(&self.lossy_stderr());
    }
}

impl OutputExtension for Output {
    fn lossy_stdout(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.stdout)
    }

    fn lossy_stderr(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.stderr)
    }

    fn report(self) -> Result<Self, ProcessError> {
        if self.status.success() {
            Ok(self)
        } else {
            Err(ProcessError::Fail(
                self.status,
                self.lossy_stderr().into_owned(),
            ))
        }
    }
}

pub trait OutputResultExtension {
    fn report(self, command: &str) -> Result<Output, CommandError>;
    fn debug_print(self) -> Self;
}

impl OutputResultExtension for Result<Output, std::io::Error> {
    fn report(self, command: &str) -> Result<Output, CommandError> {
        match self {
            Ok(ok) => ok.report(),
            Err(io) => Err(ProcessError::IO(io)),
        }.from(command)
    }

    fn debug_print(self) -> Self {
        match &self {
            Ok(output) => output.debug_print(),
            Err(_) => (),
        }
        self
    }
}

#[derive(Debug)]
pub enum ProcessError {
    IO(std::io::Error),
    Fail(ExitStatus, String),
}

impl ProcessError {
    fn from(self, command: &str) -> CommandError {
        CommandError {
            command: command.to_owned(),
            error: self,
        }
    }
}

pub trait ProcessResultExtension<T> {
    fn from(self, command: &str) -> Result<T, CommandError>;
}

impl<T> ProcessResultExtension<T> for Result<T, ProcessError> {
    fn from(self, command: &str) -> Result<T, CommandError> {
        match self {
            Ok(t) => Ok(t),
            Err(e) => Err(e.from(command)),
        }
    }
}

#[derive(Debug)]
pub struct CommandError {
    pub command: String,
    pub error: ProcessError,
}
