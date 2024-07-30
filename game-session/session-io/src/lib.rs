#![no_std]

use gmeta::{In, InOut, Metadata, Out};
use gstd::{string::String, vec::Vec, ActorId, Decode, Encode, TypeInfo};

pub struct SessionMetadata;

impl Metadata for SessionMetadata {
    type Init = In<ActorId>;
    type Handle = InOut<UserAction, UserEvent>;
    type Others = ();
    type Reply = ();
    type Signal = ();
    type State = Out<ProgramStatus>;
}

/// 用户发来的 Action 请求
#[derive(Debug, Clone, Encode, Decode, TypeInfo, PartialEq, Eq)]
pub enum UserAction {
    StartGame,
    GuessWord { word: String },
}

/// 回复用户的 Event
#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub enum UserEvent {
    Result {
        user_status: UserStatus,
        correct_position: Option<String>,
        contained_in_word: Option<String>,
        max_tries: u32,
        tries: Option<u32>,
        time_out: Option<bool>,
    },
}

/// 检查用户状态
#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub struct CheckGameStatus {
    pub user: ActorId,
}

/// 发给 Wordle 合约的 Action
#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub enum Action {
    StartGame { user: ActorId },
    CheckWord { user: ActorId, word: String },
}

/// Wordle 合约返回的 Event
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

#[derive(Debug, Clone, Encode, Decode, TypeInfo, PartialEq, Eq)]
pub enum UserStatus {
    /// 游戏未开始，不与用户关联
    GameNotStarted,
    /// 游戏已开始
    GameStarted,
    /// 游戏结束
    GameOver(GameOver),
}

/// 游戏结束
#[derive(Debug, Clone, Encode, Decode, TypeInfo, PartialEq, Eq)]
pub enum GameOver {
    /// 用户赢了
    Win,
    /// 用户输了
    Lose,
}

#[derive(Debug, Clone, Encode, Decode, TypeInfo)]
pub struct ProgramStatus {
    pub user_status_list: Option<Vec<(ActorId, UserStatus)>>,
    pub word_length: Option<u32>,
    pub max_tries: Option<u32>,
    pub max_blocks: Option<u32>,
}
