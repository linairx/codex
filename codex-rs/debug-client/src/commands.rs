#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    Message(String),
    Command(UserCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserCommand {
    Help,
    Quit,
    NewThread,
    Resume(String),
    Use(String),
    RefreshThread(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    EmptyCommand,
    MissingArgument {
        command: &'static str,
        name: &'static str,
    },
    UnknownCommand {
        command: String,
    },
}

impl ParseError {
    pub fn message(&self) -> String {
        match self {
            Self::EmptyCommand => "empty command after ':'".to_string(),
            Self::MissingArgument {
                command: "resume",
                name,
            } => format!(
                "missing required argument: {name} (use :resume <thread-id> to resume or reconnect)"
            ),
            Self::MissingArgument {
                command: "use",
                name,
            } => format!(
                "missing required argument: {name} (use :use <thread-id> to switch the active thread without resuming/reconnecting)"
            ),
            Self::MissingArgument { command, name } => {
                format!("missing required argument for :{command}: {name}")
            }
            Self::UnknownCommand { command } => format!("unknown command: {command}"),
        }
    }
}

pub fn parse_input(line: &str) -> Result<Option<InputAction>, ParseError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let Some(command_line) = trimmed.strip_prefix(':') else {
        return Ok(Some(InputAction::Message(trimmed.to_string())));
    };

    let mut parts = command_line.split_whitespace();
    let Some(command) = parts.next() else {
        return Err(ParseError::EmptyCommand);
    };

    match command {
        "help" | "h" => Ok(Some(InputAction::Command(UserCommand::Help))),
        "quit" | "q" | "exit" => Ok(Some(InputAction::Command(UserCommand::Quit))),
        "new" => Ok(Some(InputAction::Command(UserCommand::NewThread))),
        "resume" => {
            let thread_id = parts.next().ok_or(ParseError::MissingArgument {
                command: "resume",
                name: "thread-id",
            })?;
            Ok(Some(InputAction::Command(UserCommand::Resume(
                thread_id.to_string(),
            ))))
        }
        "use" => {
            let thread_id = parts.next().ok_or(ParseError::MissingArgument {
                command: "use",
                name: "thread-id",
            })?;
            Ok(Some(InputAction::Command(UserCommand::Use(
                thread_id.to_string(),
            ))))
        }
        "refresh-thread" => Ok(Some(InputAction::Command(UserCommand::RefreshThread(
            parts.next().map(ToString::to_string),
        )))),
        _ => Err(ParseError::UnknownCommand {
            command: command.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::InputAction;
    use super::ParseError;
    use super::UserCommand;
    use super::parse_input;

    #[test]
    fn parses_message() {
        let result = parse_input("hello there").unwrap();
        assert_eq!(
            result,
            Some(InputAction::Message("hello there".to_string()))
        );
    }

    #[test]
    fn parses_help_command() {
        let result = parse_input(":help").unwrap();
        assert_eq!(result, Some(InputAction::Command(UserCommand::Help)));
    }

    #[test]
    fn parses_new_thread() {
        let result = parse_input(":new").unwrap();
        assert_eq!(result, Some(InputAction::Command(UserCommand::NewThread)));
    }

    #[test]
    fn parses_resume() {
        let result = parse_input(":resume thr_123").unwrap();
        assert_eq!(
            result,
            Some(InputAction::Command(UserCommand::Resume(
                "thr_123".to_string()
            )))
        );
    }

    #[test]
    fn parses_use() {
        let result = parse_input(":use thr_456").unwrap();
        assert_eq!(
            result,
            Some(InputAction::Command(UserCommand::Use(
                "thr_456".to_string()
            )))
        );
    }

    #[test]
    fn parses_refresh_thread() {
        let result = parse_input(":refresh-thread").unwrap();
        assert_eq!(
            result,
            Some(InputAction::Command(UserCommand::RefreshThread(None)))
        );
    }

    #[test]
    fn parses_refresh_thread_with_cursor() {
        let result = parse_input(":refresh-thread cursor-2").unwrap();
        assert_eq!(
            result,
            Some(InputAction::Command(UserCommand::RefreshThread(Some(
                "cursor-2".to_string()
            ))))
        );
    }

    #[test]
    fn rejects_missing_resume_arg() {
        let result = parse_input(":resume");
        assert_eq!(
            result,
            Err(ParseError::MissingArgument {
                command: "resume",
                name: "thread-id",
            })
        );
    }

    #[test]
    fn rejects_missing_use_arg() {
        let result = parse_input(":use");
        assert_eq!(
            result,
            Err(ParseError::MissingArgument {
                command: "use",
                name: "thread-id",
            })
        );
    }

    #[test]
    fn missing_resume_arg_message_mentions_resume_or_reconnect_usage() {
        assert_eq!(
            ParseError::MissingArgument {
                command: "resume",
                name: "thread-id",
            }
            .message(),
            "missing required argument: thread-id (use :resume <thread-id> to resume or reconnect)"
        );
    }

    #[test]
    fn missing_use_arg_message_mentions_switch_usage() {
        assert_eq!(
            ParseError::MissingArgument {
                command: "use",
                name: "thread-id",
            }
            .message(),
            "missing required argument: thread-id (use :use <thread-id> to switch the active thread without resuming/reconnecting)"
        );
    }
}
