use super::*;
use serde_json::to_string;

#[test]
fn test_text_entry_serialization() {
    let entry = LogEntry {
        labels: Value::Null,
        timestamp: None,
        payload: Payload::TextPayload {
            text_payload: "test entry".into(),
        },
        severity: None,
    };

    let expected = "{\"labels\":null,\"textPayload\":\"test entry\"}";
    let result = to_string(&entry).expect("serialization failed");

    assert_eq!(expected, result, "Plain text payload should serialize correctly")
}

#[test]
fn test_json_entry_serialization() {
    let entry = LogEntry {
        labels: Value::Null,
        timestamp: None,
        payload: Payload::JsonPayload {
            json_payload: json!({
                "message": "JSON test"
            })
        },
        severity: None,
    };

    let expected = "{\"labels\":null,\"jsonPayload\":{\"message\":\"JSON test\"}}";
    let result = to_string(&entry).expect("serialization failed");

    assert_eq!(expected, result, "JSOn payload should serialize correctly")
}

#[test]
fn test_plain_text_payload() {
    let message = "plain text payload".into();
    let payload = message_to_payload(Some(message));
    let expected = Payload::TextPayload {
        text_payload: "plain text payload".into(),
    };

    assert_eq!(expected, payload, "Plain text payload should be detected correctly");
}

#[test]
fn test_empty_payload() {
    let payload = message_to_payload(None);
    let expected = Payload::TextPayload {
        text_payload: "empty log entry".into(),
    };

    assert_eq!(expected, payload, "Empty payload should be handled correctly");
}

#[test]
fn test_json_payload() {
    let message = "{\"someKey\":\"someValue\", \"otherKey\": 42}".into();
    let payload = message_to_payload(Some(message));
    let expected = Payload::JsonPayload {
        json_payload: json!({
            "someKey": "someValue",
            "otherKey": 42
        })
    };

    assert_eq!(expected, payload, "JSON payload should be detected correctly");
}

#[test]
fn test_json_no_object() {
    // This message can be parsed as valid JSON, but it is not an
    // object - it should be returned as a plain-text payload.
    let message = "42".into();
    let payload = message_to_payload(Some(message));
    let expected = Payload::TextPayload {
        text_payload: "42".into(),
    };

    assert_eq!(expected, payload, "Non-object JSON payload should be plain text");
}

#[test]
fn test_parse_microseconds() {
    let input: String = "1529175149291187".into();
    let expected: DateTime<Utc> = "2018-06-16T18:52:29.291187Z"
        .to_string().parse().unwrap();

    assert_eq!(Some(expected), parse_microseconds(input));
}
