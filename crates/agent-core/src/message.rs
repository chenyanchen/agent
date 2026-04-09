use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum Message {
    #[serde(rename = "system")]
    System { content: String },
    #[serde(rename = "user")]
    User { content: String },
    #[serde(rename = "assistant")]
    Assistant {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCall>,
    },
    #[serde(rename = "tool")]
    Tool {
        tool_call_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_system_message() {
        let msg = Message::System {
            content: "You are helpful.".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["role"], "system");
        assert_eq!(parsed["content"], "You are helpful.");
    }

    #[test]
    fn serialize_user_message() {
        let msg = Message::User {
            content: "Hello!".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["role"], "user");
        assert_eq!(parsed["content"], "Hello!");
    }

    #[test]
    fn serialize_assistant_text_message() {
        let msg = Message::Assistant {
            text: Some("Hi there!".to_string()),
            tool_calls: vec![],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["role"], "assistant");
        assert_eq!(parsed["text"], "Hi there!");
        // tool_calls should be omitted when empty
        assert!(parsed.get("tool_calls").is_none());
    }

    #[test]
    fn serialize_tool_message() {
        let msg = Message::Tool {
            tool_call_id: "call_123".to_string(),
            content: "result".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["role"], "tool");
        assert_eq!(parsed["tool_call_id"], "call_123");
        assert_eq!(parsed["content"], "result");
    }

    #[test]
    fn tool_call_roundtrip() {
        let tc = ToolCall {
            id: "tc_1".to_string(),
            name: "get_weather".to_string(),
            arguments: r#"{"city":"London"}"#.to_string(),
        };
        let json = serde_json::to_string(&tc).unwrap();
        let tc2: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(tc2.id, "tc_1");
        assert_eq!(tc2.name, "get_weather");
        assert_eq!(tc2.arguments, r#"{"city":"London"}"#);
    }

    #[test]
    fn deserialize_assistant_with_tool_calls() {
        let json = r#"{
            "role": "assistant",
            "tool_calls": [{"id":"c1","name":"fn","arguments":"{}"}]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        match msg {
            Message::Assistant { text, tool_calls } => {
                assert!(text.is_none());
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].id, "c1");
            }
            _ => panic!("expected Assistant"),
        }
    }
}
