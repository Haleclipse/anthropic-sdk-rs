// Mirrors TS SDK tests/api-resources/* while using a top-level Cargo
// integration-test harness (Cargo does not auto-discover nested test files).

#[path = "api-resources/completions.rs"]
mod completions;

#[path = "api-resources/models.rs"]
mod models;

#[path = "api-resources/message_stream.rs"]
mod message_stream;

#[path = "api-resources/beta_message_stream.rs"]
mod beta_message_stream;

#[path = "api-resources/messages/messages.rs"]
mod messages_messages;

#[path = "api-resources/messages/batches.rs"]
mod messages_batches;

#[path = "api-resources/beta/models.rs"]
mod beta_models;

#[path = "api-resources/beta/files.rs"]
mod beta_files;

#[path = "api-resources/beta/types.rs"]
mod beta_types;

#[path = "api-resources/beta/skills/skills.rs"]
mod beta_skills_skills;

#[path = "api-resources/beta/skills/versions.rs"]
mod beta_skills_versions;

#[path = "api-resources/beta/messages/messages.rs"]
mod beta_messages_messages;

#[path = "api-resources/beta/messages/types.rs"]
mod beta_messages_types;

#[path = "api-resources/beta/messages/batches.rs"]
mod beta_messages_batches;
