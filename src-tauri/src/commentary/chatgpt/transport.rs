use super::sessions::{cancelled, json_reply, RESOURCE};
use crate::commentary::{http::Http, Adapter};
use lizzie_core::commentary::{
    api_key::HttpExchange,
    chatgpt::{self, failure},
    stream, ChatTransport, CommentaryFailure, Message,
};
use lizzie_core::settings::NetworkSettings;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub struct Transport {
    app: AppHandle,
    account: String,
    model: String,
    effort: Option<String>,
    generation: u64,
    http: Http,
}
impl Transport {
    pub fn new(
        app: AppHandle,
        settings: &NetworkSettings,
        cancel: Arc<AtomicBool>,
        account: String,
        model: String,
        effort: Option<String>,
    ) -> Result<Self, CommentaryFailure> {
        let generation = app.state::<Adapter>().sessions.lock().unwrap().generation;
        let http = Http::new(settings, cancel, true).map_err(|_| failure("network"))?;
        Ok(Self {
            app,
            account,
            model,
            effort,
            generation,
            http,
        })
    }
    fn check(&self, cancel: &AtomicBool) -> Result<(), CommentaryFailure> {
        cancelled(cancel)?;
        if self.app.state::<Adapter>().sessions.lock().unwrap().generation != self.generation {
            return Err(CommentaryFailure::cancelled());
        }
        Ok(())
    }
    fn token(&mut self) -> Result<String, CommentaryFailure> {
        self.app
            .state::<Adapter>()
            .sessions
            .lock()
            .unwrap()
            .access_token(&self.account, &mut self.http)
    }
}
impl ChatTransport for Transport {
    fn complete(&mut self, messages: &[Message], cancel: &AtomicBool) -> Result<String, CommentaryFailure> {
        self.check(cancel)?;
        // Capabilities and the token are deliberately reacquired for every grounded attempt.
        let token = self.token()?;
        let models = models(&mut self.http, &token)?;
        self.check(cancel)?;
        let model = models
            .iter()
            .find(|m| m.slug == self.model)
            .ok_or_else(|| failure("models"))?;
        if self
            .effort
            .as_ref()
            .is_some_and(|effort| !effort.is_empty() && !model.efforts.contains(effort))
        {
            return Err(failure("reasoning"));
        }
        let token = self.token()?;
        self.check(cancel)?;
        let body = chatgpt::request_body(&self.model, messages, self.effort.as_deref())?.to_string();
        let mut reply = self
            .http
            .post(
                &format!("{RESOURCE}/responses"),
                &chatgpt::responses_headers(&token),
                body,
                Duration::from_secs(300),
            )
            .map_err(|e| {
                if e.cancelled {
                    CommentaryFailure::cancelled()
                } else {
                    failure("network")
                }
            })?;
        self.check(cancel)?;
        if reply.status / 100 != 2 {
            return Err(json_reply(reply)
                .err()
                .map(|e| e.failure())
                .unwrap_or_else(|| failure("protocol")));
        }
        let output = stream::parse_responses_stream(reply.body.as_mut(), cancel)?;
        self.check(cancel)?;
        Ok(output)
    }
}
pub fn models(http: &mut Http, token: &str) -> Result<Vec<chatgpt::Model>, CommentaryFailure> {
    let reply = http
        .get(
            &format!("{RESOURCE}/models"),
            &[("Authorization".into(), format!("Bearer {token}"))],
            Duration::from_secs(30),
        )
        .map_err(|e| {
            if e.cancelled {
                CommentaryFailure::cancelled()
            } else {
                failure("network")
            }
        })?;
    chatgpt::parse_models(&json_reply(reply).map_err(|e| e.failure())?.to_string())
}
