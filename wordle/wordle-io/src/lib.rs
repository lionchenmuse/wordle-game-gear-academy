#![no_std]

use gmeta::{InOut, Metadata};
use gstd::{string::String, vec::Vec, ActorId, Decode, Encode, TypeInfo};

pub struct WordleMetadata;

impl Metadata for WordleMetadata {
    type Init = ();
    type Handle = InOut<Action, Event>;
    type Others = ();
    type Reply = ();
    type Signal = ();
    type State = ();
}

#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub enum Action {
    StartGame { user: ActorId },
    CheckWord { user: ActorId, word: String },
}

#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub enum Event {
    GameStarted {
        user: ActorId,
    },
    WordChecked {
        user: ActorId,
        correct_position: Vec<u8>,
        contained_in_word: Vec<u8>,
    },
}
