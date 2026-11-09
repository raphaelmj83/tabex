//! Tabex: explorador de arquivos com janelas, grupos de abas e abas.
//!
//! A crate de biblioteca existe para que os testes de interface (`tests/`) consigam
//! instanciar o aplicativo; o binário está em `main.rs`.

pub mod app;
pub mod fs;
pub mod model;
pub mod platform;
pub mod shortcuts;
pub mod ui;
pub mod update;
