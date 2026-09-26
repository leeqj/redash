pub use redash_types::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_terminal_message_serde() {
        let msg = ClientTerminalMessage::Input {
            data: "ls -la\n".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"input\""));
        assert!(json.contains("ls -la"));

        let de: ClientTerminalMessage = serde_json::from_str(&json).unwrap();
        match de {
            ClientTerminalMessage::Input { data } => assert_eq!(data, "ls -la\n"),
            _ => panic!("wrong message type"),
        }
    }

    #[test]
    fn test_server_terminal_message_serde() {
        let msg = ServerTerminalMessage::Agent {
            name: "Claude Code".to_string(),
            state: "Thinking".to_string(),
            cost_usd: Some(0.042),
            tokens: Some(1520),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"agent\""));

        let de: ServerTerminalMessage = serde_json::from_str(&json).unwrap();
        match de {
            ServerTerminalMessage::Agent {
                name,
                cost_usd,
                tokens,
                ..
            } => {
                assert_eq!(name, "Claude Code");
                assert_eq!(cost_usd, Some(0.042));
                assert_eq!(tokens, Some(1520));
            }
            _ => panic!("wrong message type"),
        }
    }
}
