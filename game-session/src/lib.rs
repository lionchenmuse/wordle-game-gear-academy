#![no_std]

use gstd::{collections::HashMap, debug, exec, msg::{self, CodecMessageFuture}, 
    scale_info::prelude::sync::Arc, string::{String, ToString}, sync::Mutex, vec::Vec, ActorId };
use session_io::{Action, CheckGameStatus, Event, GameOver, ProgramStatus, UserAction, UserEvent, UserStatus };

static mut WORDLE_ID: Option<ActorId> = None;
// 为防止出现race condition，造成状态异常，如时间超时，用户反而赢了等情况出现。
// 使用 Arc<Mutex<HashMap<ActorId, UserStatus>>> 来存储用户的状态
static mut USER_STATUS_MAP_MUTEX: Option<Arc<Mutex<HashMap<ActorId, UserStatus>>>> = None;
static mut USER_STATUS_MAP: Option<HashMap<ActorId, UserStatus>> = None;
static mut USER_TRIES_MAP: Option<HashMap<ActorId, u32>> = None;
static mut USER_CORRECT_POSITION_MAP: Option<HashMap<ActorId, Vec<u8>>> = None;
static mut USER_CONTAINED_IN_WORD_MAP: Option<HashMap<ActorId, Vec<u8>>> = None;
// 记录上一次回复给用户的 UserEvent
static mut USER_LAST_EVENT: Option<HashMap<ActorId, UserEvent>> = None;

const MAX_TRIES: u32 = 6;
const MAX_BLOCKS: u32 = 100;
const WORD_LENGTH: usize = 5;


#[gstd::async_init]
async fn init() {
    debug!("开始初始化 game session 合约...");
    let wordle_id: ActorId = msg::load().expect("Unable to load wordle id");
    debug!("wordle id: {:?}", wordle_id);
    unsafe {
        WORDLE_ID = Some(wordle_id);
        USER_STATUS_MAP_MUTEX = Some(Arc::new(Mutex::new(HashMap::new())));
        USER_STATUS_MAP = Some(HashMap::new());
        USER_TRIES_MAP = Some(HashMap::new());
        USER_CORRECT_POSITION_MAP = Some(HashMap::new());
        USER_CONTAINED_IN_WORD_MAP = Some(HashMap::new());
        USER_LAST_EVENT = Some(HashMap::new());
    }
    debug!("初始化 game session 合约完成");
}

#[gstd::async_main]
async fn main() {
    debug!("开始执行 game session 合约...");
    let user: ActorId = msg::source();
    debug!("********* user: {:?} **********", user);
    let self_id = exec::program_id();
    debug!("本合约 id： {:?}", self_id);
    
    // 为防止出现race condition，造成状态异常，如时间超时，用户反而赢了等情况出现。
    // 使用 Arc<Mutex<HashMap<ActorId, UserStatus>>> 来存储用户的状态
    let mut user_status_mutex: Arc<Mutex<HashMap<ActorId, UserStatus>>> = unsafe {
        USER_STATUS_MAP_MUTEX.as_ref().expect("User status map is not initialized").clone()
    };
    let user_status_map: &mut HashMap<ActorId, UserStatus> = unsafe {
        USER_STATUS_MAP.as_mut().expect("User status map is not initialized")
    };

    if user != self_id {
        // 如果消息是用户发来的
        let action: UserAction = msg::load().expect("Unable to decode user action");
        debug!("用户发来的消息: {:?}", action);

        // 如果用户发送的是 StartGame
        if action == UserAction::StartGame {
            debug!("开始处理 StartGame 请求...");
            handle_start(user, user_status_map, &mut user_status_mutex).await;
            debug!("处理 StartGame 请求完成");
        } else {
            // 如果用户发送的是 GuessWord
            debug!("开始处理 GuessWord 请求...");
            handle_guess(user, &action, user_status_map, &mut user_status_mutex).await;
            debug!("处理 GuessWord 请求完成");
        }
        
    } else {
        // 如果消息是自身合约发来的
        debug!("本合约自己发来消息...");
        let action: CheckGameStatus = msg::load().expect("Unable to decode check game status");
        debug!("本合约发来消息: {:?}", action);
        // 检查用户的游戏状态
        debug!("开始执行 CheckGameStatus...");
        check_game_status(action.user, user_status_map, &mut user_status_mutex).await;
        debug!("执行 CheckGameStatus 完成");
    }
    debug!("执行 game session 合约完成");
}

/// 处理用户发出的 StartGame 请求
async fn handle_start(
    user_id: ActorId,
    user_status_map: &mut HashMap<ActorId, UserStatus>,
    user_status_mutex: &mut Arc<Mutex<HashMap<ActorId, UserStatus>>>,
) {
    let wordle_id: &ActorId = unsafe { WORDLE_ID.as_ref().expect("Wordle id is not initialized") };
    debug!("wordle 合约 id： {:?}", wordle_id);
    let user_tries_map = unsafe {
        USER_TRIES_MAP
            .as_mut()
            .expect("User tries map is not initialized")
    };
    let user_last_event = unsafe {
        USER_LAST_EVENT
            .as_mut()
            .expect("User last event is not initialized")
    };
    let shared_status = Arc::clone(user_status_mutex);

    // 控制是否发 CheckGameStatus 消息
    let flag: bool;

    let current_event: UserEvent = match user_status_map.get(&user_id) {
        // 用户已存在且重复发送 StartGame请求，
        // 则将上一次回复的内容重新发给用户
        Some(UserStatus::GameStarted) => {
            debug!("用户已经开始游戏，重复发送 StartGame 请求...");
            flag = false;
            // 取出上一次回复用户的事件内容
            user_last_event
                .get(&user_id)
                .expect("User last event not found")
                .clone()
        }
        _ => {
            // 用户不存在，或者用户已经结束游戏，请求新游戏开始
            debug!("用户不存在，或者用户已经结束游戏，请求新游戏开始...");

            // 向 wordle 发送 StartGame 消息
            debug!("向 wordle 发送 StartGame 消息...");
            let future: CodecMessageFuture<Event> = msg::send_for_reply_as(
                *wordle_id,
                Action::StartGame {
                    user: user_id.clone(),
                },
                0,
                0,
            )
            .expect("Unable to send message");
            let event: Event = future.await.expect("Unable to get reply from wordle");

            // 解析消息，如果消息是 GameStarted，则修改用户的游戏状态为 GameStarted
            if let Event::GameStarted { user } = event {
                debug!("解析消息，消息是 GameStarted({:?})", user);
                // 修改该用户游戏状态为：GameStarted
                let mut status_mutex = shared_status.lock().await;
                // 向两个用户状态表中插入该用户的游戏状态：GameStarted
                status_mutex.insert(user.clone(), UserStatus::GameStarted);
                user_status_map.insert(user.clone(), UserStatus::GameStarted);
                debug!("修改用户游戏状态为 GameStarted...");

                // 将该用户的猜测次数初始化为0
                user_tries_map.insert(user.clone(), 0);
                debug!("初始化用户的猜测次数: 0");
                // 生成用户事件
                let user_event = UserEvent::Result {
                    user_status: UserStatus::GameStarted,
                    correct_position: None,
                    contained_in_word: None,
                    max_tries: MAX_TRIES,
                    tries: None,
                    time_out: None,
                };
                debug!("生成用户事件：{:?}", user_event);
                // 要发送 CheckGameStatus 消息，将 flag 设置为 true
                flag = true;
                user_event
            } else {
                debug!("解析消息，消息不是 GameStarted！报panic!");
                panic!("Unexpected event: {:?}", event);
            }
        }
    };
    debug!("回复用户事件：{:?}", current_event);
    msg::reply(current_event.clone(), 0).expect("Unable to reply");
    // 将新事件记录到用户的上一次回复记录中
    user_last_event.insert(user_id, current_event.clone());
    debug!("将新事件记录到用户的上一次回复记录中...");

    if flag {
        // 由于向自身发送延迟消息，会与gtest::System的spend_blocks() 方法冲突，
        // 改成向 Wordle 发送延迟消息，收到 Wordle 回复后，再对改用进行检查
        debug!("向 Wordle 发送延迟消息，{:?}区块后检查该用户的游戏状态...", MAX_BLOCKS);
        msg::send_delayed(exec::program_id(), CheckGameStatus { user: user_id }, 0, MAX_BLOCKS).expect("Unable to send delayed message");
        debug!("延迟消息发送完毕");
    }
}

/// 处理用户发出的 GuessWord 请求
async fn handle_guess(
    user_id: ActorId,
    action: &UserAction,
    user_status_map: &mut HashMap<ActorId, UserStatus>,
    user_status_mutex: &mut Arc<Mutex<HashMap<ActorId, UserStatus>>>,
) {
    let wordle_id: &ActorId = unsafe { WORDLE_ID.as_ref().expect("Wordle id is not initialized") };
    // 检查用户状态（不用 Mutex 来获取）
    let user_status: Option<UserStatus> = match user_status_map.get(&user_id) {
        Some(UserStatus::GameStarted) => {
            // 该用户已经开始游戏，状态正确
            debug!("用户已经开始游戏，状态正确");
            // 解析单词
            let guess_word: String = match action {
                UserAction::GuessWord { word } => word.to_string(),
                _ => panic!("Unexpected action: {:?}", action),
            };
            debug!("用户猜测的单词：{:?}", guess_word);

            // 检查用户猜测的单词是否符合规则
            check(&guess_word);
            // 向 wordle program 发送 CheckedWord 消息
            debug!("向 wordle 发送 CheckedWord 消息...");
            let future: CodecMessageFuture<Event> = msg::send_for_reply_as(
                *wordle_id,
                Action::CheckWord {
                    user: user_id.clone(),
                    word: guess_word,
                },
                0,
                0,
            )
            .expect("Unable to send message");

            let event: Event = future.await.expect("Unable to get reply from wordle");
            debug!("收到 wordle 发来的消息：{:?}", event);
            // 更新用户的猜测检查结果
            let option: Option<UserStatus> = update_position_and_contained_map(&event);

            option
        }
        Some(UserStatus::GameOver(GameOver::Win)) => {
            debug!("用户：{:?}游戏状态为 GameOver(Win)", user_id);
            Some(UserStatus::GameOver(GameOver::Win))
        }
        Some(UserStatus::GameOver(GameOver::Lose)) => {
            debug!("用户：{:?}游戏状态为 GameOver(Lose)", user_id);
            Some(UserStatus::GameOver(GameOver::Lose))
        }
        _ => {
            debug!("用户：{:?} 游戏状态为 GameNotStarted", user_id);
            None
        }
    };
    update_user_status_and_reply(&user_id, &user_status, user_status_mutex).await;
}

#[allow(unused)]
/// 检查用户的游戏状态
async fn check_game_status(
    user: ActorId, 
    user_status_map: &mut HashMap<ActorId, UserStatus>,
    user_status_mutex: &mut Arc<Mutex<HashMap<ActorId, UserStatus>>>,
) {
    debug!("开始检查用户：{:?}", user);
    let share_status = Arc::clone(&user_status_mutex);
    let mut status_mutex = share_status.lock().await;

    let user_last_event = unsafe {
        USER_LAST_EVENT
            .as_mut()
            .expect("User last event is not initialized")
    };

    match status_mutex.get(&user) {
        Some(UserStatus::GameStarted) => {
            // 如果到时间仍未完成游戏，则将该用户的游戏状态改为 GameOver(Lose)
            debug!("用户未在规定时间内完成游戏，游戏失败！");
            status_mutex.insert(user.clone(), UserStatus::GameOver(GameOver::Lose));
            user_status_map.insert(user.clone(), UserStatus::GameOver(GameOver::Lose));
            // let user_event: UserEvent = UserEvent::Result {
            //         user_status: current_status,
            //         correct_position: Some(correct_poses),
            //         contained_in_word: Some(contained_poses),
            //         max_tries: MAX_TRIES,
            //         tries: Some(*tries),
            //         time_out: Some(time_out),
            //     };
            let mut current_event = user_last_event.get(&user).expect("User last event not found").clone();
            // 修改 current_event 内的 user_status 和 time_out 字段
                
            match &mut current_event {
                UserEvent::Result { user_status, correct_position, contained_in_word, max_tries, tries, time_out } => {
                    *user_status = UserStatus::GameOver(GameOver::Lose);
                    *time_out = Some(true);
                },
                _ => (), // 如果 current_event 不是 Result 变体则不做任何事情
            };
            user_last_event.insert(user, current_event);

            debug!("用户：{:?} 游戏状态已修改为 GameOver(Lose)", user);
        }
        _ => (),
    }
    debug!("检查用户：{:?} 完成", user);
}

/// 检查用户猜测的单词是否符合规则：
/// 1. 单词长度为 WORD_LENGTH
/// 2. 单词为小写
fn check(guess_word: &str) {
    // 检查单词长度
    debug!("检查单词长度...");
    if guess_word.len() != WORD_LENGTH {
        panic!("The length of the word must be {}", WORD_LENGTH);
    }
    debug!("单词长度满足要求：{:?}", guess_word.len());
    // 检查单词是否为小写
    debug!("检查单词是否为小写...");
    if guess_word.chars().any(|c| !c.is_ascii_lowercase()) {
        panic!("The word must be all lowercase");
    }
    debug!("单词为小写：{:?}", guess_word);
}

/// 更新用户的猜测检查结果，包括：
/// 1. 用户的猜测正确的位置: user_correct_position_map
/// 2. 用户的猜测包含在单词中的位置: user_contained_in_word_map
/// 3. 用户的猜测次数: user_tries_map
fn update_position_and_contained_map(event: &Event) -> Option<UserStatus> {
    debug!("解析消息...");
    if let Event::WordChecked {
        user,
        correct_position,
        contained_in_word,
    } = event
    {
        let user_correct_position_map = unsafe {
            USER_CORRECT_POSITION_MAP
                .as_mut()
                .expect("User correct position map is not initialized")
        };
        let user_contained_in_word_map = unsafe {
            USER_CONTAINED_IN_WORD_MAP
                .as_mut()
                .expect("User contained in word map is not initialized")
        };
        // 更新用户的猜测检查结果
        debug!("更新用户的猜测检查结果，包括：用户猜对的位置和包含在单词中的位置...");
        user_correct_position_map.insert(user.clone(), correct_position.clone());
        user_contained_in_word_map.insert(user.clone(), contained_in_word.clone());

        let user_tries_map = unsafe {
            USER_TRIES_MAP
                .as_mut()
                .expect("User tries map is not initialized")
        };
        // 更新用户的猜测次数
        let tries: &mut u32 = user_tries_map.get_mut(user).expect("User tries not found");
        debug!("更新用户的猜测次数，原来的次数是：{:?}", tries);
        *tries += 1;
        debug!("更新用户的猜测次数，现在的次数是：{:?}", tries);

        Some(UserStatus::GameStarted)
    } else {
        debug!("解析消息失败，Panic!");
        panic!("Unexpected event: {:?}", event);
    }
}

/// 更新用户的游戏状态，并回复给用户
async fn update_user_status_and_reply(
    user_id: &ActorId,
    user_status: &Option<UserStatus>,
    user_status_mutex: &mut Arc<Mutex<HashMap<ActorId, UserStatus>>>,
) {
    debug!("开始更新用户的游戏状态，并回复给用户...");

    let current_event: UserEvent;
    if user_status.is_none() {
        // 如果该用户找不到，则通知用户游戏未开始
        debug!("该用户：{:?} 未存储在游戏中，未开始游戏", user_id);
        debug!("生成该用户游戏未开始事件");
        current_event = UserEvent::Result {
            user_status: UserStatus::GameNotStarted,
            correct_position: None,
            contained_in_word: None,
            max_tries: MAX_TRIES,
            tries: None,
            time_out: None,
        }
    } else {
        let user_status_map: &mut HashMap<ActorId, UserStatus> = unsafe {
            USER_STATUS_MAP
                .as_mut()
                .expect("User status map is not initialized")
        };

        let user_correct_position_map = unsafe {
            USER_CORRECT_POSITION_MAP
                .as_mut()
                .expect("User correct position map is not initialized")
        };
        // 获取用户的猜测正确的位置
        let correct_position: &Vec<u8> = user_correct_position_map
            .get(user_id)
            .expect("User correct position not found");
        debug!("获取用户的猜测正确的位置: {:?}", correct_position);

        let user_contained_in_word_map = unsafe {
            USER_CONTAINED_IN_WORD_MAP
                .as_mut()
                .expect("User contained in word map is not initialized")
        };
        // 获取用户的猜测包含在单词中的位置
        let contained_in_word: &Vec<u8> = user_contained_in_word_map
            .get(user_id)
            .expect("User contained in word not found");
        debug!("获取用户的猜测包含在单词中的位置: {:?}", contained_in_word);

        // 该用户上一次的回复记录
        let user_last_event = unsafe {
            USER_LAST_EVENT
                .as_mut()
                .expect("User last event is not initialized")
        };
        debug!("该用户上一次的回复记录: {:?}", user_last_event);

        let user_tries_map = unsafe {
            USER_TRIES_MAP
                .as_mut()
                .expect("User tries map is not initialized")
        };
        // 获取用户的猜测次数
        let tries: &mut u32 = user_tries_map
            .get_mut(user_id)
            .expect("User tries not found");
        debug!("获取用户的最新猜测次数: {:?}", tries);

        let current_status: UserStatus;
        let time_out: bool;
        let share_status = Arc::clone(&user_status_mutex);

        // 根据不同分支生成对应回复给用户的事件内容
        current_event = match user_status {
            Some(UserStatus::GameStarted) => {
                // 如果用户游戏状态为 GameStarted
                debug!("用户: {:?} 游戏状态为 GameStarted", user_id);
                debug!("准备更新用户状态...");

                // 为避免race condition造成的状态异常，如时间超时，用户反而赢了等情况出现，
                // 使用 Arc<Mutex<HashMap<ActorId, UserStatus>>> 来修改用户状态
                let mut status_mutex = share_status.lock().await;
                // 分析获取当前用户的游戏状态
                current_status = if status_mutex.get(user_id).unwrap() == &UserStatus::GameStarted {
                    time_out = false; // 未超时
                    // 检查用户是否猜对单词
                    if correct_position.len() == WORD_LENGTH {
                        // 用户猜对单词
                        debug!("用户猜对单词！");
                        // 更新该用户的游戏状态为：GameOver(Win)
                        status_mutex.insert(user_id.clone(), UserStatus::GameOver(GameOver::Win));
                        user_status_map
                            .insert(user_id.clone(), UserStatus::GameOver(GameOver::Win));
                        debug!("用户：{:?} 游戏状态已修改为 GameOver(Win)", user_id);
                        UserStatus::GameOver(GameOver::Win)
                    } else if *tries >= MAX_TRIES {
                        // 用户猜错，次数达到最大次数
                        debug!("用户猜错，且次数已达上限");
                        // 更新该用户的游戏状态为：GameOver(Lose)
                        status_mutex.insert(user_id.clone(), UserStatus::GameOver(GameOver::Lose));
                        user_status_map
                            .insert(user_id.clone(), UserStatus::GameOver(GameOver::Lose));
                        debug!("用户：{:?} 游戏状态已修改为 GameOver(Lose)", user_id);
                        UserStatus::GameOver(GameOver::Lose)
                    } else {
                        // 用户继续猜
                        debug!("用户未猜对，次数也未达到上限");
                        // 游戏状态不变
                        debug!(
                            "用户：{:?} 游戏状态不变，维持：{:?}",
                            user_id,
                            UserStatus::GameStarted
                        );
                        UserStatus::GameStarted
                    }
                } else {
                    // 如果不是 GameStarted，则说明用户游戏已结束
                    // 已经被check_game_status函数修改过状态
                    debug!("用户游戏状态已被session 合约修改，此处不再更新该用户状态");
                    debug!("设置用户游戏超时：true");
                    time_out = true;
                    status_mutex
                        .get(user_id)
                        .expect("User status not found")
                        .clone()
                };
                let correct_poses = vec_u8_to_comma_separated_string(correct_position);
                let contained_poses = vec_u8_to_comma_separated_string(contained_in_word);

                let user_event: UserEvent = UserEvent::Result {
                    user_status: current_status,
                    correct_position: Some(correct_poses),
                    contained_in_word: Some(contained_poses),
                    max_tries: MAX_TRIES,
                    tries: Some(*tries),
                    time_out: Some(time_out),
                };
                debug!("准备回复用户事件：{:?}", user_event);
                // 更新给用户的上一次回复记录
                user_last_event.insert(user_id.clone(), user_event.clone());
                debug!("该事件已存入该用户上一次回复的记录中");
                user_event
            }
            Some(UserStatus::GameOver(_)) => {
                // 如果游戏结束，仍收到用户的 GuessWord 消息
                // 则将上一次回复的内容重新发给用户
                debug!("用户：{:?} 游戏状态为 GameOver", user_id);
                debug!("该用户游戏已结束，重复发送 GuessWord 请求...");
                debug!("获取用户上一次回复的事件，准备将此事件重发给用户");
                user_last_event
                    .get(user_id)
                    .expect("User last event not found")
                    .clone()
            }
            _ => UserEvent::Result {
                user_status: UserStatus::GameNotStarted,
                correct_position: None,
                contained_in_word: None,
                max_tries: MAX_TRIES,
                tries: None,
                time_out: None,
            },
        };
    }

    debug!("回复用户事件：{:?}", current_event.clone());
    msg::reply(current_event, 0).expect("Unable to reply");
}

pub fn vec_u8_to_comma_separated_string(bytes: &Vec<u8>) -> String {
    bytes.iter()
         .map(|&b| b.to_string())
         .collect::<Vec<_>>()
         .join(",")
}



#[no_mangle]
extern "C" fn state() {
    let program_status = ProgramStatus {
        user_status_list: unsafe {
            USER_STATUS_MAP
                .as_ref()
                .map(|m| m.iter().map(|(k, v)| (*k, v.clone())).collect())
        },
        word_length: Some(WORD_LENGTH as u32),
        max_tries: Some(MAX_TRIES),
        max_blocks: Some(MAX_BLOCKS),
    };

    msg::reply(program_status, 0).expect("Unable to get program status");
}
