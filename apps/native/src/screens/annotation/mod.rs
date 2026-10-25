//! "Enviar trecho ao agente" (`DiffAnnotationPopover`): code picked in a
//! diff or a file, with what the agent should do about it.

pub mod card;
mod card_view;
pub mod trigger;

pub use card::bind_keys;

use dcc_core::domain::code_annotation::CodeAnnotation;
use gpui::{Pixels, Point};

/// Code to send, and where the card opens from: the trigger's bottom-left
/// corner, in window coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct AnnotationRequest {
    pub annotation: CodeAnnotation,
    pub anchor: Point<Pixels>,
}

#[cfg(test)]
#[path = "card_tests.rs"]
mod card_tests;
