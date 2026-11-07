use std::{
    cell::RefCell,
    collections::VecDeque,
    io::{self, BufRead, Cursor, Read},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use lizzie_core::commentary::{
    api_key::{self, ApiKeyClient, HttpExchange, HttpFailure, HttpReply},
    chatgpt::{self, ErrorCategory},
    stream, ChatTransport, Message,
};
use serde_json::{json, Value};

fn cancel() -> AtomicBool {
    AtomicBool::new(false)
}
fn messages() -> Vec<Message> {
    vec![Message {
        role: "user".into(),
        content: "Explain this move.".into(),
    }]
}
fn chat(body: &str) -> Result<String, lizzie_core::commentary::CommentaryFailure> {
    stream::parse_chat_completions(&mut Cursor::new(body), &cancel(), "secret-key")
}
fn api_responses(body: &str) -> Result<String, lizzie_core::commentary::CommentaryFailure> {
    stream::parse_api_responses(&mut Cursor::new(body), &cancel(), "secret-key")
}
fn gpt(body: &str) -> Result<String, lizzie_core::commentary::CommentaryFailure> {
    stream::parse_responses_stream(&mut Cursor::new(body), &cancel())
}
const CHAT_DELTA: &str = "data: {\"choices\":[{\"delta\":{\"content\":\"Good move.\"}}]}\n";
const API_DELTA: &str = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Good move.\"}\n";
const COMPLETE: &str = "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";

#[test]
fn chat_done_and_stop_complete_but_other_reasons_fail() {
    assert_eq!(chat(&format!("{CHAT_DELTA}data: [DONE]\n")).unwrap(), "Good move.");
    assert_eq!(
        chat("data: {\"choices\":[{\"delta\":{\"content\":\"last\"},\"finish_reason\":\"stop\"}]}\n").unwrap(),
        "last"
    );
    for reason in ["length", "content_filter", "tool_calls"] {
        let error = chat(&format!(
            "{CHAT_DELTA}data: {{\"choices\":[{{\"finish_reason\":\"{reason}\"}}]}}\n"
        ))
        .unwrap_err();
        assert_eq!(error.message_key, "Teacher.error.incomplete");
    }
    // API-key parsing itself allows empty successful text; the grounded consumer rejects it.
    assert_eq!(chat("data: [DONE]\n").unwrap(), "");
}

#[test]
fn api_responses_require_explicit_completed_status() {
    assert_eq!(api_responses(&format!("{API_DELTA}{COMPLETE}")).unwrap(), "Good move.");
    assert_eq!(
        api_responses(&format!(
            "data: {{\"type\":\"output_text_delta\",\"delta\":\"\",\"content\":\"fallback\"}}\n{COMPLETE}"
        ))
        .unwrap(),
        "fallback"
    );
    for suffix in [
        "data: [DONE]\n",
        "",
        "data: {\"type\":\"response.completed\"}\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"incomplete\"}}\n",
        "data: {\"type\":\"response.incomplete\"}\n",
        "data: {\"type\":\"response.failed\"}\n",
        "data: {\"type\":\"error\"}\n",
    ] {
        assert_eq!(
            api_responses(&format!("{API_DELTA}{suffix}")).unwrap_err().message_key,
            "Teacher.error.incomplete"
        );
    }
}

#[test]
fn api_malformed_and_eof_discard_partial_output() {
    for body in [
        "data: {broken}\n",
        "data: []\n",
        "data: null\n",
        CHAT_DELTA,
        "event: ignored\n\n",
    ] {
        let error = chat(body).unwrap_err();
        assert_eq!(error.message_key, "Teacher.error.incomplete");
        assert_eq!(
            error.message,
            "The reply was interrupted and was not saved. Please try again."
        );
        assert!(!error.cancelled);
    }
}

#[test]
fn stream_errors_redact_keys_bearer_and_json_fields() {
    let detail = " key=secret-key\nBearer other-token {\"api_key\":\"hidden1\",\"api-key\":\"hidden2\",\"apikey\":\"hidden3\",\"Authorization\":\"hidden4\",\"token\":\"hidden5\"}";
    let safe = stream::sanitize_error(detail, "secret-key");
    for secret in [
        "secret-key",
        "other-token",
        "hidden1",
        "hidden2",
        "hidden3",
        "hidden4",
        "hidden5",
    ] {
        assert!(!safe.contains(secret));
    }
    assert!(safe.contains("Bearer [redacted]"));
    assert!(!safe.contains('\n'));
    for event in [
        json!({"error":{"message":detail}}),
        json!({"type":"response.failed","response":{"error":{"message":detail}}}),
    ] {
        let error = api_responses(&format!("data: {event}\n")).unwrap_err();
        assert_eq!(error.message, format!("AI service stream failed: {safe}"));
        assert_eq!(error.message_key, "");
    }
    assert_eq!(
        chat("data: {\"error\":{}}\n").unwrap_err().message,
        "AI service stream failed."
    );
    let error = chat(&format!("data: {}\n", json!({"error":{"message":"a".repeat(241)}}))).unwrap_err();
    assert_eq!(
        error.message,
        format!("AI service stream failed: {}...", "a".repeat(240))
    );
    assert_eq!(stream::sanitize_error("  x\u{00a0}y\t z  ", ""), "x\u{00a0}y z");
}

struct CancellingReader<'a> {
    body: Cursor<Vec<u8>>,
    cancel: &'a AtomicBool,
    lines: usize,
}
impl Read for CancellingReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.body.read(buf)
    }
}
impl BufRead for CancellingReader<'_> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.body.fill_buf()
    }
    fn consume(&mut self, n: usize) {
        self.body.consume(n);
    }
    fn read_line(&mut self, buf: &mut String) -> io::Result<usize> {
        let size = self.body.read_line(buf)?;
        self.lines += 1;
        if self.lines == 2 {
            self.cancel.store(true, Ordering::Release);
        }
        Ok(size)
    }
}

#[test]
fn cancellation_is_checked_per_line_and_before_reading() {
    let flag = cancel();
    let mut reader = CancellingReader {
        body: Cursor::new(format!("{CHAT_DELTA}data: [DONE]\n").into_bytes()),
        cancel: &flag,
        lines: 0,
    };
    assert!(
        stream::parse_chat_completions(&mut reader, &flag, "")
            .unwrap_err()
            .cancelled
    );
    let flag = cancel();
    let mut reader = CancellingReader {
        body: Cursor::new(format!("{API_DELTA}\n{COMPLETE}").into_bytes()),
        cancel: &flag,
        lines: 0,
    };
    assert!(
        stream::parse_responses_stream(&mut reader, &flag)
            .unwrap_err()
            .cancelled
    );
    flag.store(true, Ordering::Release);
    assert!(
        stream::parse_api_responses(&mut Cursor::new(""), &flag, "")
            .unwrap_err()
            .cancelled
    );
}

#[derive(Debug)]
struct Call {
    method: &'static str,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<Value>,
    timeout: Duration,
}
struct FakeHttp {
    replies: VecDeque<(u16, String)>,
    calls: Rc<RefCell<Vec<Call>>>,
}
impl FakeHttp {
    fn new(replies: Vec<(u16, String)>) -> (Self, Rc<RefCell<Vec<Call>>>) {
        let calls = Rc::new(RefCell::new(Vec::new()));
        (
            Self {
                replies: replies.into(),
                calls: calls.clone(),
            },
            calls,
        )
    }
    fn reply(&mut self) -> Result<HttpReply, HttpFailure> {
        let (status, body) = self.replies.pop_front().expect("unexpected extra HTTP request");
        Ok(HttpReply {
            status,
            body: Box::new(Cursor::new(body.into_bytes())),
        })
    }
}
impl HttpExchange for FakeHttp {
    fn get(&mut self, url: &str, headers: &[(String, String)], timeout: Duration) -> Result<HttpReply, HttpFailure> {
        self.calls.borrow_mut().push(Call {
            method: "GET",
            url: url.into(),
            headers: headers.into(),
            body: None,
            timeout,
        });
        self.reply()
    }
    fn post(
        &mut self,
        url: &str,
        headers: &[(String, String)],
        body: String,
        timeout: Duration,
    ) -> Result<HttpReply, HttpFailure> {
        self.calls.borrow_mut().push(Call {
            method: "POST",
            url: url.into(),
            headers: headers.into(),
            body: Some(serde_json::from_str(&body).unwrap()),
            timeout,
        });
        self.reply()
    }
}

#[test]
fn only_404_and_405_fall_back_once_with_correct_request_shapes() {
    for status in [404, 405] {
        let (http, calls) = FakeHttp::new(vec![
            (status, "ignored".into()),
            (200, format!("{API_DELTA}{COMPLETE}")),
        ]);
        let mut client = ApiKeyClient::new("https://h", " key ", " model ")
            .unwrap()
            .with_exchange(http);
        assert_eq!(client.complete(&messages(), &cancel()).unwrap(), "Good move.");
        let calls = calls.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].url, "https://h/v1/chat/completions");
        assert_eq!(calls[1].url, "https://h/v1/responses");
        for call in calls.iter() {
            assert_eq!(call.method, "POST");
            assert_eq!(call.timeout, Duration::from_secs(300));
            for (name, value) in [
                ("Authorization", "Bearer key"),
                ("Content-Type", "application/json; charset=utf-8"),
                ("User-Agent", "LizzieYzy-Next-AI-Commentary"),
                ("Accept", "text/event-stream"),
            ] {
                assert!(call.headers.iter().any(|(k, v)| k == name && v == value));
            }
        }
        assert_eq!(
            calls[0].body.as_ref().unwrap(),
            &json!({"model":"model","stream":true,"messages":[{"role":"user","content":"Explain this move."}]})
        );
        assert_eq!(
            calls[1].body.as_ref().unwrap(),
            &json!({"model":"model","stream":true,"input":[{"role":"user","content":"Explain this move."}]})
        );
    }
    for status in [301, 400, 401, 429, 500] {
        let (http, calls) = FakeHttp::new(vec![(status, "Bearer exposed key".into())]);
        let mut client = ApiKeyClient::new("https://h/api", "key", "m")
            .unwrap()
            .with_exchange(http);
        let error = client.complete(&messages(), &cancel()).unwrap_err();
        assert_eq!(
            error.message,
            format!("AI service returned HTTP {status}: Bearer [redacted] [redacted]")
        );
        assert_eq!(calls.borrow().len(), 1);
    }
    let (http, calls) = FakeHttp::new(vec![(404, String::new()), (405, String::new())]);
    let mut client = ApiKeyClient::new("https://h", "key", "m").unwrap().with_exchange(http);
    assert_eq!(
        client.complete(&messages(), &cancel()).unwrap_err().message,
        "AI service returned HTTP 405."
    );
    assert_eq!(calls.borrow().len(), 2);
}

#[test]
fn network_failure_does_not_retry_and_is_sanitized() {
    struct NetworkFailure(usize);
    impl HttpExchange for NetworkFailure {
        fn get(&mut self, _: &str, _: &[(String, String)], _: Duration) -> Result<HttpReply, HttpFailure> {
            panic!("unexpected GET");
        }
        fn post(&mut self, _: &str, _: &[(String, String)], _: String, _: Duration) -> Result<HttpReply, HttpFailure> {
            self.0 += 1;
            assert_eq!(self.0, 1);
            Err(HttpFailure {
                message: "failed Bearer secret".into(),
                cancelled: false,
            })
        }
    }
    let mut client = ApiKeyClient::new("https://h", "secret", "m")
        .unwrap()
        .with_exchange(NetworkFailure(0));
    assert_eq!(
        client.complete(&messages(), &cancel()).unwrap_err().message,
        "failed Bearer [redacted]"
    );
}

#[test]
fn base_normalization_and_constructor_validation() {
    for (input, expected) in [
        ("https://h", "https://h/v1"),
        ("https://h/", "https://h/v1"),
        ("https://h/api", "https://h/api"),
        ("https://h/v1/", "https://h/v1"),
        (" http://localhost:8080/// ", "http://localhost:8080/v1"),
    ] {
        let client = ApiKeyClient::new(input, "key", "model").unwrap();
        assert_eq!(client.base_url(), expected);
        assert_eq!(api_key::normalize_api_base(input).unwrap(), expected);
    }
    for url in [
        "http://h",
        "https://u:p@h",
        "https://h?x=y",
        "https://h/#x",
        "",
        "https://h/%zz",
    ] {
        assert!(ApiKeyClient::new(url, "key", "m").is_err());
    }
    for key in ["", " \t", "x\u{7f}y", "x\ny"] {
        assert!(ApiKeyClient::new("https://h", key, "m").is_err());
    }
    for model in ["".to_owned(), "x".repeat(161), "x\u{85}".into()] {
        assert!(ApiKeyClient::new("https://h", "key", &model).is_err());
    }
}

#[test]
fn api_message_limits_roles_and_blank_messages() {
    let (http, calls) = FakeHttp::new(vec![(200, "data: [DONE]\n".into())]);
    let mut client = ApiKeyClient::new("https://h", "key", "m").unwrap().with_exchange(http);
    let error = client
        .complete(
            &[Message {
                role: "user".into(),
                content: "x".repeat(250_001),
            }],
            &cancel(),
        )
        .unwrap_err();
    assert_eq!(error.message, "AI commentary context is too large.");
    assert!(calls.borrow().is_empty());
    assert_eq!(
        client.complete(&[], &cancel()).unwrap_err().message,
        "At least one message is required."
    );
    assert_eq!(
        client
            .complete(
                &[Message {
                    role: "ignored".into(),
                    content: " \t".into()
                }],
                &cancel()
            )
            .unwrap_err()
            .message,
        "At least one non-empty message is required."
    );
    assert_eq!(
        client
            .complete(
                &[Message {
                    role: "developer".into(),
                    content: "x".into()
                }],
                &cancel()
            )
            .unwrap_err()
            .message,
        "Unsupported message role."
    );
    client
        .complete(
            &[
                Message {
                    role: "ignored".into(),
                    content: " ".into(),
                },
                Message {
                    role: " system ".into(),
                    content: "x".repeat(250_000),
                },
            ],
            &cancel(),
        )
        .unwrap();
    assert_eq!(
        calls.borrow()[0].body.as_ref().unwrap()["messages"][0]["role"],
        "system"
    );
    let mut client = ApiKeyClient::new("https://h", "key", "m")
        .unwrap()
        .with_exchange(FakeHttp::new(vec![]).0);
    assert_eq!(
        client
            .complete(
                &[Message {
                    role: "user".into(),
                    content: "😀".repeat(125_001)
                }],
                &cancel()
            )
            .unwrap_err()
            .message,
        "AI commentary context is too large."
    );
}

#[test]
fn discovery_trims_deduplicates_and_uses_java_natural_order() {
    let (http, calls) = FakeHttp::new(vec![(200, json!({"data":[{"id":" z "},{"id":"a2"},{"id":"a10"},{"id":"z"},{"id":" "},{},null,{"id":"😀"},{"id":"\u{e000}"}]}).to_string())]);
    let mut client = ApiKeyClient::for_models("https://h/api", "key")
        .unwrap()
        .with_exchange(http);
    assert_eq!(client.list_models().unwrap(), vec!["a10", "a2", "z", "😀", "\u{e000}"]);
    let calls = calls.borrow();
    assert_eq!(calls[0].method, "GET");
    assert_eq!(calls[0].url, "https://h/api/models");
    assert_eq!(calls[0].timeout, Duration::from_secs(30));
    assert!(!calls[0].headers.iter().any(|(name, _)| name == "Accept"));
    assert_eq!(api_key::parse_models("{}").unwrap(), Vec::<String>::new());
    assert!(api_key::parse_models("broken").is_err());
}

#[test]
fn chatgpt_models_preserve_server_order_and_filter_capabilities() {
    let models = chatgpt::parse_models(&json!({"models":[
        {"visibility":"hidden","slug":"hidden"},
        {"visibility":"list","slug":"second","display_name":"Second","supported_reasoning_levels":[{"effort":"high"},{"effort":"low"},{"effort":"high"},{"effort":"High"},{"effort":"x bad"},{"effort":"a________________________________"}],"default_reasoning_level":"high"},
        {"visibility":"list","slug":"second","display_name":"duplicate"},
        {"visibility":"list","slug":"first","default_reasoning_level":"high"},
        {"visibility":"list","slug":""}, {"visibility":"list","slug":"  "},
        {"visibility":"list","slug":"bad\u{85}"}, {"visibility":"list","slug":"x".repeat(161)}
    ]}).to_string()).unwrap();
    assert_eq!(
        models.iter().map(|m| m.slug.as_str()).collect::<Vec<_>>(),
        vec!["second", "first"]
    );
    assert_eq!(models[0].display_name, "Second");
    assert_eq!(models[0].efforts, vec!["high", "low"]);
    assert_eq!(models[0].default_effort, "high");
    assert_eq!(models[1].display_name, "first");
    assert_eq!(models[1].default_effort, "");
    assert_eq!(
        chatgpt::parse_models("{}").unwrap_err().message_key,
        "Teacher.chatgpt.error.protocol"
    );
    assert_eq!(
        chatgpt::parse_models("{\"models\":[]}").unwrap_err().message_key,
        "Teacher.chatgpt.error.models"
    );
}

#[test]
fn chatgpt_request_body_maps_roles_and_has_no_extra_controls() {
    let input = vec![
        Message {
            role: "system".into(),
            content: "rules".into(),
        },
        Message {
            role: "developer".into(),
            content: "more".into(),
        },
        Message {
            role: "user".into(),
            content: "question".into(),
        },
        Message {
            role: "assistant".into(),
            content: "answer".into(),
        },
        Message {
            role: "ignored".into(),
            content: " ".into(),
        },
    ];
    let body = chatgpt::request_body("m", &input, Some("high")).unwrap();
    assert_eq!(
        body,
        json!({"model":"m","input":[{"role":"developer","content":"rules"},{"role":"developer","content":"more"},{"role":"user","content":"question"},{"role":"assistant","content":"answer"}],"store":false,"stream":true,"reasoning":{"effort":"high"}})
    );
    assert!(chatgpt::request_body("m", &input, None)
        .unwrap()
        .get("reasoning")
        .is_none());
    assert_eq!(
        chatgpt::request_body("m", &input, Some("HIGH"))
            .unwrap_err()
            .message_key,
        "Teacher.chatgpt.error.reasoning"
    );
    assert_eq!(
        chatgpt::request_body("", &input, None).unwrap_err().message_key,
        "Teacher.chatgpt.error.models"
    );
    assert_eq!(
        chatgpt::request_body(
            "m",
            &[Message {
                role: " system ".into(),
                content: "x".into()
            }],
            None
        )
        .unwrap_err()
        .message_key,
        "Teacher.chatgpt.error.protocol"
    );
    assert!(chatgpt::request_body(
        "m",
        &[Message {
            role: "user".into(),
            content: "x".repeat(300_001)
        }],
        None
    )
    .is_err());
    assert!(chatgpt::request_body(
        "m",
        &[Message {
            role: "user".into(),
            content: "x".repeat(300_000)
        }],
        None
    )
    .is_ok());
    assert_eq!(
        chatgpt::responses_headers("token"),
        vec![
            ("Authorization".into(), "Bearer token".into()),
            ("Content-Type".into(), "application/json".into()),
            ("Accept".into(), "text/event-stream".into())
        ]
    );
}

#[test]
fn chatgpt_error_classification_matches_code_overrides_and_status_defaults() {
    use ErrorCategory::*;
    let cases = [
        (500, "subscription_sharing_usage_limit_exceeded", Limit),
        (429, "subscription_sharing_user_not_eligible", Ineligible),
        (401, "chatpass_v2_scope_not_authorized", Permission),
        (500, "chatpass_v2_invalid_authorization_context", Permission),
        (401, "subscription_sharing_unsupported_capability", Protocol),
        (500, "subscription_sharing_route_not_supported", Protocol),
        (500, "invalid_client", Protocol),
        (500, "invalid_grant", LoginRequired),
        (500, "invalid_refresh_token", LoginRequired),
        (500, "token_expired", LoginRequired),
        (500, "refresh_token_expired", LoginRequired),
        (500, "refresh_token_invalidated", LoginRequired),
        (500, "refresh_token_reused", LoginRequired),
        (500, "subscription_sharing_invalid_user", LoginRequired),
        (403, "subscription_sharing_usage_unavailable", Network),
        (403, "subscription_sharing_user_unavailable", Network),
        (429, "other", Limit),
        (401, "other", LoginRequired),
        (403, "other", Permission),
        (400, "other", Protocol),
        (500, "other", Network),
        (0, "", Network),
    ];
    for (status, code, category) in cases {
        let error = chatgpt::classify_error(
            status,
            &json!({"error":{"code":code,"param":"model","message":"secret URL https://provider/"}}),
            "req.id:42-1",
        );
        assert_eq!(error.category, category, "{status}/{code}");
        assert_eq!(error.status, status);
        assert_eq!(error.code, code);
        assert_eq!(error.parameter, "model");
        assert_eq!(error.request_id, "req.id:42-1");
        assert_eq!(error.failure().message_key, category.message_key());
        assert!(!error.failure().message.contains("secret"));
        assert!(chatgpt::is_known_error(&error.failure().message));
    }
    let error = chatgpt::classify_error(
        403,
        &json!({"error":{"code":"unsafe code","param":"https://secret/"}}),
        "Bearer secret",
    );
    assert_eq!(error.code, "");
    assert_eq!(error.parameter, "");
    assert_eq!(error.request_id, "");
    assert_eq!(error.category, Permission);
    assert_eq!(
        chatgpt::response_error(401, "not json", "request").category,
        LoginRequired
    );
    assert!(chatgpt::classify_error(0, &json!({"error":"invalid_grant"}), "").refresh_revoked());
    assert!(!chatgpt::classify_error(0, &json!({"error":"subscription_sharing_invalid_user"}), "").refresh_revoked());
}

#[test]
fn chatgpt_sse_joins_data_lines_and_requires_nonempty_explicit_completion() {
    let multiline =
        "event: message\ndata: {\"type\":\"response.output_text.delta\",\ndata:  \"delta\":\"Good move.\"}\n\n";
    assert_eq!(gpt(&format!("{multiline}{COMPLETE}")).unwrap(), "Good move.");
    for body in [
        format!("{API_DELTA}\n"),
        format!("{API_DELTA}\ndata: [DONE]\n\n"),
        COMPLETE.into(),
        "data: {broken}\n\n".into(),
        "data: {\"type\":\"response.incomplete\"}\n\n".into(),
        format!("data: {{\"type\":\"output_text_delta\",\"delta\":\"ignored\"}}\n\n{COMPLETE}"),
        format!("data: {{\"type\":\"response.output_text.delta\",\"content\":\"ignored\"}}\n\n{COMPLETE}"),
    ] {
        assert_eq!(gpt(&body).unwrap_err().message_key, "Teacher.chatgpt.error.incomplete");
    }
    let event = json!({"type":"response.failed","response":{"error":{"code":"subscription_sharing_usage_limit_exceeded","message":"token secret"}}});
    let error = gpt(&format!("data: {event}\n\n")).unwrap_err();
    assert_eq!(error.message_key, "Teacher.chatgpt.error.limit");
    assert!(!error.message.contains("secret"));
}

#[test]
fn chatgpt_caps_event_and_accumulated_output_at_one_mib_utf16_units() {
    let oversized_event = json!({"type":"response.output_text.delta","delta":"x".repeat(1024*1024)});
    assert_eq!(
        gpt(&format!("data: {oversized_event}\n\n")).unwrap_err().message_key,
        "Teacher.chatgpt.error.protocol"
    );
    let chunk = json!({"type":"response.output_text.delta","delta":"x".repeat(512*1024)});
    let body = format!("data: {chunk}\n\ndata: {chunk}\n\n{COMPLETE}");
    assert_eq!(gpt(&body).unwrap().len(), 1024 * 1024);
    let body = format!("data: {chunk}\n\ndata: {chunk}\n\ndata: {{\"type\":\"response.output_text.delta\",\"delta\":\"x\"}}\n\n{COMPLETE}");
    assert_eq!(gpt(&body).unwrap_err().message_key, "Teacher.chatgpt.error.protocol");
}

#[test]
fn http_error_detail_is_bounded_and_sanitized() {
    let (http, _) = FakeHttp::new(vec![(500, format!("{} secret-key", "a".repeat(5000)))]);
    let mut client = ApiKeyClient::new("https://h", "secret-key", "m")
        .unwrap()
        .with_exchange(http);
    assert_eq!(
        client.complete(&messages(), &cancel()).unwrap_err().message,
        format!("AI service returned HTTP 500: {}...", "a".repeat(240))
    );
}

#[test]
fn java_optional_json_strings_preserve_scalar_coercion() {
    assert_eq!(
        api_key::parse_models(r#"{"data":[{"id":42},{"id":"42"},{"id":true}]}"#).unwrap(),
        vec!["42", "true"]
    );
    let models = chatgpt::parse_models(r#"{"models":[{"visibility":"list","slug":42,"display_name":true}]}"#).unwrap();
    assert_eq!(models[0].slug, "42");
    assert_eq!(models[0].display_name, "true");
    let error = chatgpt::classify_error(500, &json!({"error":{"code":42,"param":true}}), "");
    assert_eq!(error.code, "42");
    assert_eq!(error.parameter, "true");
    assert_eq!(
        chat("data: {\"choices\":[{\"delta\":{\"content\":42}}]}\ndata: [DONE]\n").unwrap(),
        "42"
    );
}
