//! redislock is an implementation of the [distributed locking
//! mechanism](http://redis.io/topics/distlock) built on top of Redis.
//!
//! It is more or less a port of the [Ruby version](https://github.com/antirez/redlock-rb).
//!
//! # Basic Operation
//! ```rust,no_run
//! # use redislock::{random_char, RedisLock};
//! let rl = RedisLock::new(vec![
//!     "redis://127.0.0.1:6380/",
//!     "redis://127.0.0.1:6381/",
//!     "redis://127.0.0.1:6382/"]);
//!
//! let lock;
//! loop {
//!   let val = random_char(Some(20));
//!   match rl.lock("mutex".as_bytes(), val, 1000, None, None) {
//!     Some(l) => { lock = l; break; }
//!     None => ()
//!   }
//! }
//!
//! // Critical section
//!
//! rl.unlock(&lock);
//! ```

mod redislock;

use rand::Rng;
pub use crate::redislock::{Lock, RedisLock};

const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                            abcdefghijklmnopqrstuvwxyz\
                            0123456789)(*&^%$#@!~";

const DEFAULT_RANDOM_LEN: usize = 20;

pub fn random_char(len: Option<usize>) -> Vec<u8> {
    let mut rng = rand::thread_rng();
    let length: usize;
    if let Some(len) = len {
        if len > 0 {
            length = len
        } else {
            length = DEFAULT_RANDOM_LEN
        }
    } else { length = DEFAULT_RANDOM_LEN }

    let words: Vec<u8> = (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as u8
        })
        .collect();
    words
}

#[cfg(test)]
mod test {
    use anyhow::Result;
    use crate::random_char;

    #[test]
    fn test_random_char() -> Result<()> {
        let words1 = random_char(Some(20));
        println!("{:?}", words1);
        let words2 = random_char(Some(20));
        println!("{:?}", words2);
        assert_eq!(20, words1.len());
        assert_eq!(20, words2.len());
        assert_ne!(words1, words2);
        println!("{}", String::from_utf8(words1).unwrap());
        println!("{}", String::from_utf8(words2).unwrap());
        Ok(())
    }
}