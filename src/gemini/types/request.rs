use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[allow(non_camel_case_types)]
pub enum Role {
    user,
    model,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[allow(non_camel_case_types)]
pub enum Part {
    text(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Chat {
    pub(crate) role: Role,
    pub(crate) parts: Vec<Part>,
}

impl Chat {
    pub fn new(role: Role, parts: Vec<Part>) -> Self {
        Self { role, parts }
    }
    pub fn parts(&self) -> &Vec<Part> {
        &self.parts
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct SystemInstruction {
    parts: Vec<Part>,
}

impl SystemInstruction {
    pub fn from_str(prompt: impl Into<String>) -> Self {
        Self {
            parts: vec![Part::text(prompt.into())],
        }
    }
}

#[derive(Serialize)]
pub struct GeminiRequestBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<&'a SystemInstruction>,
    contents: &'a [&'a Chat],
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<&'a Value>,
}

impl<'a> GeminiRequestBody<'a> {
    pub fn new(
        system_instruction: Option<&'a SystemInstruction>,
        contents: &'a [&'a Chat],
        generation_config: Option<&'a Value>,
    ) -> Self {
        Self {
            system_instruction,
            contents,
            generation_config,
        }
    }
}