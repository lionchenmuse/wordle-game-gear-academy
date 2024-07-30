#![no_std]

use gstd::{collections::HashMap, debug, exec, msg, string::String, vec::Vec, ActorId, ToString};
use wordle_io::{Action, Event};

static mut WORDLE: Option<Wordle> = None;
// const BANK_OR_WORDS: [&str; 6] = ["house", "human", "horse", "humor", "happy", "hello"];
const BANK_OR_WORDS: [&str; 1] = ["house"];
const WORD_LENGTH: usize = 5;

#[derive(Default)]
struct Wordle {
    games: HashMap<ActorId, String>,
}

#[no_mangle]
extern "C" fn init() {
    debug!("开始初始化 Wordle 合约");
    unsafe {
        WORDLE = Some(Wordle {
            games: HashMap::new(),
        });
    }
    debug!("初始化 Wordle 合约完成");
}

#[no_mangle]
extern "C" fn handle() {
    let action: Action = msg::load().expect("Unable to decode action");
    let wordle: &mut Wordle =
        unsafe { WORDLE.as_mut().expect("Wordle program is not initialized") };

    let reply = match action {
        Action::StartGame { user } => {
            // 随机抽取一个单词，并将用户id 与 单词一起存入 games
            // 即这个单词是该用户要猜的单词
            let random_id = get_random_value(BANK_OR_WORDS.len() as u8);
            let word = BANK_OR_WORDS[random_id as usize]; // const 变量不需要使用 unsafe
            wordle.games.insert(user, word.to_string()); // 这里的 to_string() 方法，要先引入 gstd::ToString，不是 gstd::string::ToString
            Event::GameStarted { user }
        }
        Action::CheckWord { user, word } => {
            if word.len() != WORD_LENGTH {
                panic!("The length of the word must be {}", WORD_LENGTH);
            }
            // 取出该用户要猜的单词
            let key_word = wordle
                .games
                .get(&user)
                .expect("There is no game with this user");
            let mut matched_indices = Vec::with_capacity(WORD_LENGTH);
            let mut key_indices = Vec::with_capacity(WORD_LENGTH);

            // 比较待猜的单词，和用户输入的单词
            for (i, (a, b)) in key_word.chars().zip(word.chars()).enumerate() {
                if a == b {
                    // 如果同一索引位置，字符相同，则将索引存入 matched_indices
                    matched_indices.push(i as u8);
                } else if key_word.contains(b) {
                    // 如果字符不同，但是待猜的单词包含用户输入的字符，
                    // 则将索引存入 key_indices
                    key_indices.push(i as u8);
                }
            }
            Event::WordChecked {
                user,
                correct_position: matched_indices,
                contained_in_word: key_indices,
            }
        },
    };
    msg::reply(reply, 0).expect("Error in sending a reply");
}

static mut SEED: u8 = 0;
pub fn get_random_value(range: u8) -> u8 {
    let seed = unsafe { SEED };
    unsafe {
        SEED = SEED.wrapping_add(1);
    }

    let mut random_input: [u8; 32] = exec::program_id().into();
    random_input[0] = random_input[0].wrapping_add(seed);

    let (random, _) = exec::random(random_input).expect("Error in getting random number");
    random[0] % range
}