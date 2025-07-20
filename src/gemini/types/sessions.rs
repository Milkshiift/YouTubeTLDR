use super::request::*;
use super::response::GeminiResponse;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::mem::discriminant;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Session {
    history: VecDeque<Chat>,
    history_limit: usize,
    chat_no: usize,
    remember_reply: bool,
}

impl Session {
    /// `history_limit`: Total number of chat of user and model allowed.
    /// ## Example
    /// new(2) will allow only 1 question and 1 reply to be stored.
    pub fn new(history_limit: usize) -> Self {
        Self {
            history: VecDeque::new(),
            history_limit,
            chat_no: 0,
            remember_reply: true,
        }
    }
    pub fn get_history_limit(&self) -> usize {
        self.history_limit
    }
    pub fn get_history(&self) -> Vec<&Chat> {
        let (left, right) = self.history.as_slices();
        left.iter().chain(right.iter()).collect()
    }
    pub fn get_history_length(&self) -> usize {
        self.history.len()
    }
    pub fn get_remember_reply(&self) -> bool {
        self.remember_reply
    }
    fn add_chat(&mut self, mut chat: Chat) -> &mut Self {
        if let Some(last_chat) = self.history.back_mut() {
            if discriminant(&last_chat.role) == discriminant(&chat.role) {
                last_chat.parts.append(&mut chat.parts);
                return self;
            }
        }

        self.history.push_back(chat);
        self.chat_no += 1;
        if self.get_history_length() > self.get_history_limit() {
            self.history.pop_front();
        }
        self
    }
    /// If ask_string is called more than once without passing through `gemini.ask(&mut session)`
    /// or `session.reply("opportunist")`, the prompt string is concatenated with the previous prompt.
    pub fn ask_string(&mut self, prompt: impl Into<String>) -> &mut Self {
        self.add_chat(Chat::new(Role::user, vec![Part::text(prompt.into())]))
    }
    pub(crate) fn update<'b>(&mut self, response: &'b GeminiResponse) -> Option<&'b Vec<Part>> {
        if self.get_remember_reply() {
            let reply_parts = response.get_parts();
            self.add_chat(Chat::new(Role::model, reply_parts.clone()));
            Some(reply_parts)
        } else {
            if let Some(chat) = self.history.back() {
                if let Role::user = chat.role {
                    self.history.pop_back();
                }
            }
            None
        }
    }
}