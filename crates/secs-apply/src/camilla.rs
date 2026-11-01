use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;
use tungstenite::{Message, WebSocket};

use crate::ApplyError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    GetVersion,
    GetState,
    GetStopReason,
    GetConfigTitle,
    GetConfigDescription,
    GetConfigFilePath,
    GetCaptureRate,
    GetClippedSamples,
    GetProcessingLoad,
    GetSignalLevels,
    ValidateConfig(String),
    SetConfigFilePath(String),
    Reload,
    Exit,
}

impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Self::GetVersion => "GetVersion",
            Self::GetState => "GetState",
            Self::GetStopReason => "GetStopReason",
            Self::GetConfigTitle => "GetConfigTitle",
            Self::GetConfigDescription => "GetConfigDescription",
            Self::GetConfigFilePath => "GetConfigFilePath",
            Self::GetCaptureRate => "GetCaptureRate",
            Self::GetClippedSamples => "GetClippedSamples",
            Self::GetProcessingLoad => "GetProcessingLoad",
            Self::GetSignalLevels => "GetSignalLevels",
            Self::ValidateConfig(_) => "ValidateConfig",
            Self::SetConfigFilePath(_) => "SetConfigFilePath",
            Self::Reload => "Reload",
            Self::Exit => "Exit",
        }
    }

    /// Returns the JSON text frame accepted by CamillaDSP.
    pub fn to_message(&self) -> String {
        match self {
            Self::ValidateConfig(value) | Self::SetConfigFilePath(value) => {
                let mut object = serde_json::Map::new();
                object.insert(self.name().to_owned(), Value::String(value.clone()));
                Value::Object(object).to_string()
            }
            _ => Value::String(self.name().to_owned()).to_string(),
        }
    }
}

fn unexpected(command: &Command, text: &str) -> ApplyError {
    ApplyError::Reply {
        command: command.name().to_owned(),
        reply: text.to_owned(),
    }
}

fn error_message(result: &Value, body: &serde_json::Map<String, Value>) -> Option<String> {
    if result.as_str() == Some("Error") {
        return Some(
            body.get("value")
                .and_then(Value::as_str)
                .unwrap_or("Error")
                .to_owned(),
        );
    }
    if let Some(name) = result.as_str() {
        return (name != "Ok").then(|| name.to_owned());
    }
    let result = result.as_object()?;
    if result.len() != 1 {
        return None;
    }
    let (name, message) = result.iter().next()?;
    Some(
        message
            .as_str()
            .map_or_else(|| name.clone(), ToOwned::to_owned),
    )
}

/// Parses both CamillaDSP 3.x's `Ok`/`Error` replies and 4.x's named errors.
pub fn parse_reply(command: &Command, text: &str) -> Result<Value, ApplyError> {
    let parsed: Value = serde_json::from_str(text).map_err(|_| unexpected(command, text))?;
    let object = parsed
        .as_object()
        .ok_or_else(|| unexpected(command, text))?;

    if object
        .get("Invalid")
        .and_then(Value::as_object)
        .and_then(|invalid| invalid.get("error"))
        .and_then(Value::as_str)
        .is_some()
    {
        return Err(ApplyError::Unsupported {
            command: command.name().to_owned(),
        });
    }

    let body = object
        .get(command.name())
        .and_then(Value::as_object)
        .ok_or_else(|| unexpected(command, text))?;
    let result = body
        .get("result")
        .ok_or_else(|| unexpected(command, text))?;

    if result.as_str() == Some("Ok") {
        return Ok(body.get("value").cloned().unwrap_or(Value::Null));
    }
    if let Some(message) = error_message(result, body) {
        return Err(ApplyError::Rejected {
            command: command.name().to_owned(),
            message,
        });
    }
    Err(unexpected(command, text))
}

pub trait Transport {
    /// Sends one text frame and returns the next text frame.
    fn request(&mut self, message: &str) -> Result<String, ApplyError>;
}

#[derive(Debug)]
struct DeadlineStream {
    stream: TcpStream,
    deadline: Instant,
}

impl DeadlineStream {
    fn new(stream: TcpStream, timeout: Duration) -> Self {
        Self {
            stream,
            deadline: Instant::now() + timeout,
        }
    }

    fn set_deadline(&mut self, deadline: Instant) {
        self.deadline = deadline;
    }

    fn remaining(&self) -> io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(timed_out)
    }

    fn map_timeout(&self, error: io::Error) -> io::Error {
        if matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ) && Instant::now() >= self.deadline
        {
            timed_out()
        } else {
            error
        }
    }
}

impl Read for DeadlineStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.stream.set_read_timeout(Some(self.remaining()?))?;
        self.stream
            .read(buffer)
            .map_err(|error| self.map_timeout(error))
    }
}

impl Write for DeadlineStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        self.stream
            .write(buffer)
            .map_err(|error| self.map_timeout(error))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        self.stream.flush().map_err(|error| self.map_timeout(error))
    }
}

fn timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "websocket request timed out")
}

#[derive(Debug)]
pub struct WsTransport {
    socket: WebSocket<DeadlineStream>,
    timeout: Duration,
}

impl WsTransport {
    /// Connects to plain `ws://host:port` and applies one timeout to connect,
    /// websocket handshake, reads, and writes.
    pub fn connect(addr: &str, timeout: Duration) -> Result<Self, ApplyError> {
        let addresses = addr
            .to_socket_addrs()
            .map_err(|error| ApplyError::Connect {
                addr: addr.to_owned(),
                message: error.to_string(),
                connection_refused: false,
            })?;
        let mut last_error = None;
        let mut stream = None;
        for address in addresses {
            match TcpStream::connect_timeout(&address, timeout) {
                Ok(connected) => {
                    stream = Some(connected);
                    break;
                }
                Err(error) => last_error = Some(error),
            }
        }
        let connection_refused = last_error
            .as_ref()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::ConnectionRefused);
        let stream = stream.ok_or_else(|| ApplyError::Connect {
            addr: addr.to_owned(),
            message: last_error.map_or_else(
                || "address resolved to no socket addresses".to_owned(),
                |error| error.to_string(),
            ),
            connection_refused,
        })?;
        let stream = DeadlineStream::new(stream, timeout);
        let (socket, _) = tungstenite::client(format!("ws://{addr}"), stream).map_err(|error| {
            ApplyError::Connect {
                addr: addr.to_owned(),
                message: error.to_string(),
                connection_refused: false,
            }
        })?;
        Ok(Self { socket, timeout })
    }
}

impl Transport for WsTransport {
    fn request(&mut self, message: &str) -> Result<String, ApplyError> {
        let deadline = Instant::now() + self.timeout;
        self.socket.get_mut().set_deadline(deadline);
        self.socket
            .send(Message::Text(message.to_owned().into()))
            .map_err(|error| ApplyError::Transport(error.to_string()))?;
        loop {
            match self
                .socket
                .read()
                .map_err(|error| ApplyError::Transport(error.to_string()))?
            {
                Message::Text(text) => return Ok(text.to_string()),
                Message::Ping(_) | Message::Pong(_) => {}
                Message::Close(frame) => {
                    return Err(ApplyError::Transport(format!(
                        "connection closed before reply: {frame:?}"
                    )));
                }
                Message::Binary(_) | Message::Frame(_) => {
                    return Err(ApplyError::Transport(
                        "received a non-text websocket reply".to_owned(),
                    ));
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CamillaStatus {
    pub version: String,
    pub state: String,
    pub stop_reason: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub config_path: Option<String>,
    pub capture_rate: Option<u32>,
    pub clipped_samples: Option<u64>,
    pub processing_load_pct: Option<f64>,
    pub playback_peak_db: Option<Vec<f64>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ApplyOutcome {
    Applied {
        previous_path: Option<String>,
    },
    RolledBack {
        error: String,
        previous_path: Option<String>,
    },
}

#[derive(Debug)]
pub struct Camilla<T: Transport> {
    transport: T,
}

impl<T: Transport> Camilla<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    fn command(&mut self, command: &Command) -> Result<Value, ApplyError> {
        let reply = self.transport.request(&command.to_message())?;
        parse_reply(command, &reply)
    }

    fn optional_command(&mut self, command: &Command) -> Result<Option<Value>, ApplyError> {
        match self.command(command) {
            Ok(value) => Ok(Some(value)),
            Err(ApplyError::Unsupported { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn required_string(&mut self, command: &Command) -> Result<String, ApplyError> {
        let value = self.command(command)?;
        value
            .as_str()
            .map(ToOwned::to_owned)
            .ok_or_else(|| ApplyError::Reply {
                command: command.name().to_owned(),
                reply: value.to_string(),
            })
    }

    /// Reads the status fields supported by the running CamillaDSP.
    pub fn status(&mut self) -> Result<CamillaStatus, ApplyError> {
        let version = self.required_string(&Command::GetVersion)?;
        let state = self.required_string(&Command::GetState)?;
        let stop_reason = optional_string(
            self.optional_command(&Command::GetStopReason)?,
            &Command::GetStopReason,
        )?;
        let title = optional_string(
            self.optional_command(&Command::GetConfigTitle)?,
            &Command::GetConfigTitle,
        )?;
        let description = optional_string(
            self.optional_command(&Command::GetConfigDescription)?,
            &Command::GetConfigDescription,
        )?;
        let config_path = optional_string(
            self.optional_command(&Command::GetConfigFilePath)?,
            &Command::GetConfigFilePath,
        )?;
        let capture_rate = optional_u64(
            self.optional_command(&Command::GetCaptureRate)?,
            &Command::GetCaptureRate,
        )?
        .map(|value| {
            u32::try_from(value).map_err(|_| ApplyError::Reply {
                command: Command::GetCaptureRate.name().to_owned(),
                reply: value.to_string(),
            })
        })
        .transpose()?;
        let clipped_samples = optional_u64(
            self.optional_command(&Command::GetClippedSamples)?,
            &Command::GetClippedSamples,
        )?;
        let processing_load_pct = optional_f64(
            self.optional_command(&Command::GetProcessingLoad)?,
            &Command::GetProcessingLoad,
        )?;
        let playback_peak_db = optional_playback_peak(
            self.optional_command(&Command::GetSignalLevels)?,
            &Command::GetSignalLevels,
        )?;

        Ok(CamillaStatus {
            version,
            state,
            stop_reason,
            title,
            description,
            config_path,
            capture_rate,
            clipped_samples,
            processing_load_pct,
            playback_peak_db,
        })
    }

    /// Reads the CamillaDSP version as a small identity probe.
    pub fn version(&mut self) -> Result<String, ApplyError> {
        self.required_string(&Command::GetVersion)
    }

    /// Asks the connected CamillaDSP process to exit.
    pub fn exit(&mut self) -> Result<(), ApplyError> {
        self.command(&Command::Exit).map(|_| ())
    }

    /// Validates, selects, and reloads a configuration, restoring the previous
    /// path if the reload is rejected.
    pub fn apply(
        &mut self,
        config_path: &str,
        config_text: &str,
    ) -> Result<ApplyOutcome, ApplyError> {
        let config_dir = Path::new(config_path).parent().unwrap_or(Path::new(""));
        self.command(&Command::ValidateConfig(with_absolute_filenames(
            config_text,
            config_dir,
        )))?;
        let previous_value = self.command(&Command::GetConfigFilePath)?;
        let previous_path = value_as_optional_string(previous_value, &Command::GetConfigFilePath)?;
        if let Err(select_error) = self.command(&Command::SetConfigFilePath(config_path.to_owned()))
        {
            // A reply that never came may still have changed the selection, so
            // put the previous path back and report it either way: the caller
            // keeps that generation (audit #155 TD-108).
            let Some(path) = previous_path else {
                return Err(select_error);
            };
            let restore = self.command(&Command::SetConfigFilePath(path.clone()));
            return Err(ApplyError::SelectionUncertain {
                message: match restore {
                    Ok(_) => format!("{select_error}; previous path restored"),
                    Err(restore_error) => {
                        format!(
                            "{select_error}; restoring the previous path failed: {restore_error}"
                        )
                    }
                },
                previous_path: path,
            });
        }

        match self.command(&Command::Reload) {
            Ok(_) => Ok(ApplyOutcome::Applied { previous_path }),
            Err(reload_error) => {
                let error = reload_error.to_string();
                if let Some(path) = &previous_path {
                    let rollback = self
                        .command(&Command::SetConfigFilePath(path.clone()))
                        .and_then(|_| self.command(&Command::Reload));
                    if let Err(rollback_error) = rollback {
                        return Err(ApplyError::RollbackFailed {
                            command: "Reload".into(),
                            message: format!("{error}; rollback failed: {rollback_error}"),
                            previous_path: path.clone(),
                        });
                    }
                }
                Ok(ApplyOutcome::RolledBack {
                    error,
                    previous_path,
                })
            }
        }
    }
}

fn value_as_optional_string(value: Value, command: &Command) -> Result<Option<String>, ApplyError> {
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_str()
        .map(|value| Some(value.to_owned()))
        .ok_or_else(|| ApplyError::Reply {
            command: command.name().to_owned(),
            reply: value.to_string(),
        })
}

fn optional_string(value: Option<Value>, command: &Command) -> Result<Option<String>, ApplyError> {
    value
        .map(|value| value_as_optional_string(value, command))
        .transpose()
        .map(Option::flatten)
}

fn optional_u64(value: Option<Value>, command: &Command) -> Result<Option<u64>, ApplyError> {
    value
        .map(|value| {
            value.as_u64().ok_or_else(|| ApplyError::Reply {
                command: command.name().to_owned(),
                reply: value.to_string(),
            })
        })
        .transpose()
}

fn optional_f64(value: Option<Value>, command: &Command) -> Result<Option<f64>, ApplyError> {
    value
        .map(|value| {
            value.as_f64().ok_or_else(|| ApplyError::Reply {
                command: command.name().to_owned(),
                reply: value.to_string(),
            })
        })
        .transpose()
}

fn optional_playback_peak(
    value: Option<Value>,
    command: &Command,
) -> Result<Option<Vec<f64>>, ApplyError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let peaks = value
        .as_object()
        .and_then(|levels| levels.get("playback_peak"))
        .and_then(Value::as_array)
        .ok_or_else(|| ApplyError::Reply {
            command: command.name().to_owned(),
            reply: value.to_string(),
        })?;
    peaks
        .iter()
        .map(|peak| {
            peak.as_f64().ok_or_else(|| ApplyError::Reply {
                command: command.name().to_owned(),
                reply: value.to_string(),
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

/// Returns `config_text` with every relative `"filename":"…"` made absolute
/// under `config_dir`.
///
/// The exporter writes coefficient paths relative to the config file so a
/// bundle can move (`secs-camilla`). CamillaDSP resolves them against the
/// config file's folder when it loads the file (`SetConfigFilePath`,
/// `Reload`), but `ValidateConfig` receives only text and resolves them
/// against its own working directory, which failed every FIR filter with
/// "The system cannot find the path specified" (manage-e2e, 2026-10-30).
/// Only the validated copy changes; the file on disk keeps relative paths.
pub fn with_absolute_filenames(config_text: &str, config_dir: &Path) -> String {
    const KEY: &str = "\"filename\":";
    let mut out = String::with_capacity(config_text.len());
    let mut rest = config_text;
    while let Some(at) = rest.find(KEY) {
        let (before, after_key) = rest.split_at(at + KEY.len());
        out.push_str(before);
        let trimmed = after_key.trim_start_matches(' ');
        out.push_str(&after_key[..after_key.len() - trimmed.len()]);
        let mut stream = serde_json::Deserializer::from_str(trimmed).into_iter::<String>();
        match stream.next() {
            Some(Ok(value)) if !Path::new(&value).is_absolute() && !value.is_empty() => {
                let absolute = value
                    .split('/')
                    .fold(config_dir.to_path_buf(), |path, part| path.join(part));
                out.push_str(
                    &serde_json::to_string(&absolute.to_string_lossy())
                        .expect("a string serializes"),
                );
                rest = &trimmed[stream.byte_offset()..];
            }
            _ => rest = trimmed,
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::net::TcpListener;
    use std::thread;
    use std::time::{Duration, Instant};

    use tungstenite::protocol::frame::{
        Frame,
        coding::{Data, OpCode},
    };

    use super::*;

    #[derive(Debug, Default)]
    struct FakeTransport {
        replies: VecDeque<Result<String, ApplyError>>,
        sent: Vec<String>,
    }

    impl FakeTransport {
        fn from_replies(replies: impl IntoIterator<Item = String>) -> Self {
            Self {
                replies: replies.into_iter().map(Ok).collect(),
                sent: Vec::new(),
            }
        }
    }

    impl Transport for FakeTransport {
        fn request(&mut self, message: &str) -> Result<String, ApplyError> {
            self.sent.push(message.to_owned());
            self.replies
                .pop_front()
                .expect("fake transport must have a reply")
        }
    }

    fn ok(command: &str, value: Value) -> String {
        serde_json::json!({command: {"result": "Ok", "value": value}}).to_string()
    }

    fn ok_without_value(command: &str) -> String {
        serde_json::json!({command: {"result": "Ok"}}).to_string()
    }

    #[test]
    fn messages_cover_every_command() {
        let commands = [
            (Command::GetVersion, "\"GetVersion\""),
            (Command::GetState, "\"GetState\""),
            (Command::GetStopReason, "\"GetStopReason\""),
            (Command::GetConfigTitle, "\"GetConfigTitle\""),
            (Command::GetConfigDescription, "\"GetConfigDescription\""),
            (Command::GetConfigFilePath, "\"GetConfigFilePath\""),
            (Command::GetCaptureRate, "\"GetCaptureRate\""),
            (Command::GetClippedSamples, "\"GetClippedSamples\""),
            (Command::GetProcessingLoad, "\"GetProcessingLoad\""),
            (Command::GetSignalLevels, "\"GetSignalLevels\""),
            (Command::Reload, "\"Reload\""),
        ];
        for (command, expected) in commands {
            assert_eq!(command.to_message(), expected);
        }
        assert_eq!(
            Command::ValidateConfig("title: \"room\"\n".into()).to_message(),
            r#"{"ValidateConfig":"title: \"room\"\n"}"#
        );
        assert_eq!(
            Command::SetConfigFilePath(r"C:\SECS\room.yml".into()).to_message(),
            r#"{"SetConfigFilePath":"C:\\SECS\\room.yml"}"#
        );
    }

    #[test]
    fn replies_cover_ok_v3_error_v4_error_unknown_and_malformed() {
        assert_eq!(
            parse_reply(
                &Command::GetState,
                r#"{"GetState":{"result":"Ok","value":"Running"}}"#
            )
            .unwrap(),
            Value::String("Running".into())
        );
        assert!(matches!(
            parse_reply(
                &Command::ValidateConfig(String::new()),
                r#"{"ValidateConfig":{"result":"Error","value":"bad yaml"}}"#
            ),
            Err(ApplyError::Rejected { message, .. }) if message == "bad yaml"
        ));
        assert!(matches!(
            parse_reply(
                &Command::Reload,
                r#"{"Reload":{"result":{"ConfigValidationError":"bad config"}}}"#
            ),
            Err(ApplyError::Rejected { message, .. }) if message == "bad config"
        ));
        assert!(matches!(
            parse_reply(
                &Command::GetSignalLevels,
                r#"{"Invalid":{"error":"unknown variant `GetSignalLevels`"}}"#
            ),
            Err(ApplyError::Unsupported { .. })
        ));
        assert!(matches!(
            parse_reply(&Command::GetState, "not json"),
            Err(ApplyError::Reply { .. })
        ));
    }

    #[test]
    fn status_reads_v4_replies() {
        let replies = [
            ok("GetVersion", serde_json::json!("4.1.3")),
            ok("GetState", serde_json::json!("Running")),
            ok("GetStopReason", serde_json::json!("None")),
            ok("GetConfigTitle", serde_json::json!("SECS room")),
            ok(
                "GetConfigDescription",
                serde_json::json!("secs-design 0123"),
            ),
            ok("GetConfigFilePath", serde_json::json!("room.yml")),
            ok("GetCaptureRate", serde_json::json!(48000)),
            ok("GetClippedSamples", serde_json::json!(7)),
            ok("GetProcessingLoad", serde_json::json!(12.5)),
            ok(
                "GetSignalLevels",
                serde_json::json!({
                    "playback_rms": [-20.0, -21.0],
                    "playback_peak": [-3.0, -4.0],
                    "capture_rms": [-30.0, -31.0],
                    "capture_peak": [-10.0, -11.0]
                }),
            ),
        ];
        let mut camilla = Camilla::new(FakeTransport::from_replies(replies));
        assert_eq!(
            camilla.status().unwrap(),
            CamillaStatus {
                version: "4.1.3".into(),
                state: "Running".into(),
                stop_reason: Some("None".into()),
                title: Some("SECS room".into()),
                description: Some("secs-design 0123".into()),
                config_path: Some("room.yml".into()),
                capture_rate: Some(48_000),
                clipped_samples: Some(7),
                processing_load_pct: Some(12.5),
                playback_peak_db: Some(vec![-3.0, -4.0]),
            }
        );
    }

    #[test]
    fn status_leaves_unknown_optional_commands_empty() {
        let unknown = r#"{"Invalid":{"error":"unknown variant"}}"#.to_owned();
        let replies = [
            ok("GetVersion", serde_json::json!("3.0.0")),
            ok("GetState", serde_json::json!("Running")),
            unknown.clone(),
            unknown.clone(),
            unknown.clone(),
            ok("GetConfigFilePath", Value::Null),
            unknown.clone(),
            unknown.clone(),
            unknown.clone(),
            unknown,
        ];
        let mut camilla = Camilla::new(FakeTransport::from_replies(replies));
        assert_eq!(
            camilla.status().unwrap(),
            CamillaStatus {
                version: "3.0.0".into(),
                state: "Running".into(),
                stop_reason: None,
                title: None,
                description: None,
                config_path: None,
                capture_rate: None,
                clipped_samples: None,
                processing_load_pct: None,
                playback_peak_db: None,
            }
        );
    }

    #[test]
    fn apply_succeeds_in_the_required_order() {
        let replies = [
            ok("ValidateConfig", serde_json::json!("validated")),
            ok("GetConfigFilePath", serde_json::json!("old.yml")),
            ok_without_value("SetConfigFilePath"),
            ok_without_value("Reload"),
        ];
        let mut camilla = Camilla::new(FakeTransport::from_replies(replies));
        assert_eq!(
            camilla.apply("new.yml", "title: new").unwrap(),
            ApplyOutcome::Applied {
                previous_path: Some("old.yml".into())
            }
        );
        assert_eq!(
            camilla.transport.sent,
            [
                r#"{"ValidateConfig":"title: new"}"#,
                r#""GetConfigFilePath""#,
                r#"{"SetConfigFilePath":"new.yml"}"#,
                r#""Reload""#,
            ]
        );
    }

    #[test]
    fn rejected_validation_sends_nothing_else() {
        let replies = [r#"{"ValidateConfig":{"result":"Error","value":"bad"}}"#.to_owned()];
        let mut camilla = Camilla::new(FakeTransport::from_replies(replies));
        assert!(matches!(
            camilla.apply("new.yml", "bad"),
            Err(ApplyError::Rejected { .. })
        ));
        assert_eq!(camilla.transport.sent.len(), 1);
    }

    #[test]
    fn failed_reload_restores_and_reloads_the_previous_path() {
        let replies = [
            ok("ValidateConfig", serde_json::json!("validated")),
            ok("GetConfigFilePath", serde_json::json!("old.yml")),
            ok_without_value("SetConfigFilePath"),
            r#"{"Reload":{"result":"Error","value":"cannot start"}}"#.to_owned(),
            ok_without_value("SetConfigFilePath"),
            ok_without_value("Reload"),
        ];
        let mut camilla = Camilla::new(FakeTransport::from_replies(replies));
        assert_eq!(
            camilla.apply("new.yml", "title: new").unwrap(),
            ApplyOutcome::RolledBack {
                error: "Reload: cannot start".into(),
                previous_path: Some("old.yml".into()),
            }
        );
        assert_eq!(
            &camilla.transport.sent[4..],
            [r#"{"SetConfigFilePath":"old.yml"}"#, r#""Reload""#]
        );
    }

    #[test]
    fn failed_rollback_carries_both_reload_errors() {
        let replies = [
            ok("ValidateConfig", serde_json::json!("validated")),
            ok("GetConfigFilePath", serde_json::json!("old.yml")),
            ok_without_value("SetConfigFilePath"),
            r#"{"Reload":{"result":"Error","value":"cannot start"}}"#.to_owned(),
            r#"{"SetConfigFilePath":{"result":"Error","value":"cannot restore"}}"#.to_owned(),
        ];
        let mut camilla = Camilla::new(FakeTransport::from_replies(replies));
        assert!(matches!(
            camilla.apply("new.yml", "title: new"),
            Err(ApplyError::RollbackFailed {
                command,
                message,
                previous_path,
            }) if command == "Reload"
                && previous_path == "old.yml"
                && message.contains("cannot start")
                && message.contains("rollback failed")
                && message.contains("cannot restore")
        ));
    }

    #[test]
    fn unanswered_selection_restores_and_reports_the_previous_path() {
        let mut transport = FakeTransport::from_replies([
            ok("ValidateConfig", serde_json::json!("validated")),
            ok("GetConfigFilePath", serde_json::json!("old.yml")),
        ]);
        transport
            .replies
            .push_back(Err(ApplyError::Transport("request timed out".into())));
        transport
            .replies
            .push_back(Ok(ok_without_value("SetConfigFilePath")));
        let mut camilla = Camilla::new(transport);
        assert!(matches!(
            camilla.apply("new.yml", "title: new"),
            Err(ApplyError::SelectionUncertain {
                message,
                previous_path,
            }) if previous_path == "old.yml"
                && message.contains("request timed out")
                && message.contains("previous path restored")
        ));
        assert_eq!(camilla.transport.sent.len(), 4);
        assert!(camilla.transport.sent[3].contains("old.yml"));
    }

    #[test]
    fn failed_reload_without_previous_path_reports_without_restore_commands() {
        let replies = [
            ok("ValidateConfig", serde_json::json!("validated")),
            ok("GetConfigFilePath", Value::Null),
            ok_without_value("SetConfigFilePath"),
            r#"{"Reload":{"result":"Error","value":"cannot start"}}"#.to_owned(),
        ];
        let mut camilla = Camilla::new(FakeTransport::from_replies(replies));
        assert_eq!(
            camilla.apply("new.yml", "title: new").unwrap(),
            ApplyOutcome::RolledBack {
                error: "Reload: cannot start".into(),
                previous_path: None,
            }
        );
        assert_eq!(camilla.transport.sent.len(), 4);
    }

    #[test]
    fn camilla_ws() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            assert_eq!(socket.read().unwrap().to_text().unwrap(), r#""GetState""#);
            socket
                .send(Message::Text(
                    r#"{"GetState":{"result":"Ok","value":"Running"}}"#.into(),
                ))
                .unwrap();
        });

        let mut transport =
            WsTransport::connect(&addr.to_string(), Duration::from_secs(2)).unwrap();
        assert_eq!(
            transport.request(r#""GetState""#).unwrap(),
            r#"{"GetState":{"result":"Ok","value":"Running"}}"#
        );
        server.join().unwrap();
    }

    #[test]
    fn websocket_ping_frames_do_not_extend_request_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            let _ = socket.read().unwrap();
            let deadline = Instant::now() + Duration::from_millis(400);
            while Instant::now() < deadline {
                if socket.send(Message::Ping(Vec::new().into())).is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
        });

        let mut transport =
            WsTransport::connect(&addr.to_string(), Duration::from_secs(2)).unwrap();
        transport.timeout = Duration::from_millis(100);
        let started = Instant::now();
        assert!(matches!(
            transport.request(r#""GetState""#),
            Err(ApplyError::Transport(_))
        ));
        assert!(started.elapsed() < Duration::from_millis(300));
        server.join().unwrap();
    }

    #[test]
    fn incomplete_fragment_stream_does_not_extend_request_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            let _ = socket.read().unwrap();
            if socket
                .send(Message::Frame(Frame::message(
                    b"{".as_slice(),
                    OpCode::Data(Data::Text),
                    false,
                )))
                .is_err()
            {
                return;
            }
            let deadline = Instant::now() + Duration::from_millis(400);
            while Instant::now() < deadline {
                thread::sleep(Duration::from_millis(20));
                if socket
                    .send(Message::Frame(Frame::message(
                        b" ".as_slice(),
                        OpCode::Data(Data::Continue),
                        false,
                    )))
                    .is_err()
                {
                    break;
                }
            }
        });

        let mut transport =
            WsTransport::connect(&addr.to_string(), Duration::from_secs(2)).unwrap();
        transport.timeout = Duration::from_millis(100);
        let started = Instant::now();
        assert!(matches!(
            transport.request(r#""GetState""#),
            Err(ApplyError::Transport(_))
        ));
        assert!(started.elapsed() < Duration::from_millis(300));
        server.join().unwrap();
    }

    #[test]
    fn websocket_read_obeys_the_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut socket = tungstenite::accept(stream).unwrap();
            let _ = socket.read().unwrap();
            thread::sleep(Duration::from_millis(400));
        });

        let mut transport =
            WsTransport::connect(&addr.to_string(), Duration::from_secs(2)).unwrap();
        transport.timeout = Duration::from_millis(100);
        let started = Instant::now();
        assert!(matches!(
            transport.request(r#""GetState""#),
            Err(ApplyError::Transport(_))
        ));
        assert!(started.elapsed() < Duration::from_secs(2));
        server.join().unwrap();
    }

    #[test]
    fn validation_text_gets_absolute_coefficient_paths() {
        let dir = Path::new("/cfg");
        let text = "filters:\n  in_L_fir: {\"type\":\"Conv\",\"parameters\":{\"type\":\"Wav\",\"filename\":\"filters/secs_mixed_L.wav\",\"channel\":0}}\n  in_R_fir: {\"type\":\"Conv\",\"parameters\":{\"type\":\"Wav\",\"filename\": \"/abs/R.wav\"}}\n";
        let fixed = with_absolute_filenames(text, dir);
        let expected = serde_json::to_string(
            &Path::new("/cfg")
                .join("filters")
                .join("secs_mixed_L.wav")
                .to_string_lossy(),
        )
        .unwrap();
        assert!(
            fixed.contains(&format!("\"filename\":{expected},\"channel\":0")),
            "{fixed}"
        );
        let kept = serde_json::to_string(
            &std::env::temp_dir()
                .join("abs")
                .join("R.wav")
                .to_string_lossy(),
        )
        .unwrap();
        let text = format!("{{\"filename\": {kept}}}");
        assert_eq!(with_absolute_filenames(&text, dir), text);
        assert_eq!(fixed.matches("\"filename\":").count(), 2);
        assert_eq!(with_absolute_filenames("title: x\n", dir), "title: x\n");
        assert_eq!(
            with_absolute_filenames("{\"filename\":12}", dir),
            "{\"filename\":12}"
        );
    }
}
