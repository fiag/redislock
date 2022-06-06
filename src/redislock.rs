use std::fs::File;
use std::io::{self, Read};
use std::thread::sleep;
use std::time::{Duration, Instant};

use rand::{thread_rng, Rng};
use redis::Value::Okay;
use redis::{Client, IntoConnectionInfo, RedisResult, Value};

const DEFAULT_RETRY_COUNT: u32 = 3;
const DEFAULT_RETRY_DELAY: u32 = 200;
const CLOCK_DRIFT_FACTOR: f32 = 0.01;
const UNLOCK_SCRIPT: &str = r"if redis.call('get',KEYS[1]) == ARGV[1] then
                                return redis.call('del',KEYS[1])
                              else
                                return 0
                              end";

/// The lock manager.
///
/// Implements the necessary functionality to acquire and release locks
/// and handles the Redis connections.
#[derive(Debug, Clone)]
pub struct RedisLock {
    /// List of all Redis clients
    pub servers: Vec<Client>,
    quorum: u32,
    retry_count: u32,
    retry_delay: u32,
}

pub struct Lock<'a> {
    /// The resource to lock. Will be used as the key in Redis.
    pub resource: Vec<u8>,
    /// The value for this lock.
    pub val: Vec<u8>,
    /// Time the lock is still valid.
    /// Should only be slightly smaller than the requested TTL.
    pub validity_time: usize,
    /// Used to limit the lifetime of a lock to its lock manager.
    pub lock_manager: &'a RedisLock,
}

pub struct RedisLockGuard<'a> {
    pub lock: Lock<'a>,
}

impl Drop for RedisLockGuard<'_> {
    fn drop(&mut self) {
        self.lock.lock_manager.unlock(&self.lock);
    }
}

impl RedisLock {
    /// Create a new lock manager instance, defined by the given Redis connection uris.
    /// Quorum is defined to be N/2+1, with N being the number of given Redis instances.
    ///
    /// Sample URI: `"redis://127.0.0.1:6379"`
    pub fn new<T: AsRef<str> + IntoConnectionInfo>(uris: Vec<T>) -> RedisLock {
        let quorum = (uris.len() as u32) / 2 + 1;

        let servers: Vec<Client> = uris
            .into_iter()
            .map(|uri| Client::open(uri).unwrap())
            .collect();

        RedisLock {
            servers,
            quorum,
            retry_count: DEFAULT_RETRY_COUNT,
            retry_delay: DEFAULT_RETRY_DELAY,
        }
    }

    /// get quorum
    pub fn quorum(&self) -> u32 {
        self.quorum
    }

    /// Get 20 random bytes from `/dev/urandom`.
    pub fn get_unique_lock_id(&self) -> io::Result<Vec<u8>> {
        let file = File::open("/dev/urandom")?;
        let mut buf = Vec::with_capacity(20);
        match file.take(20).read_to_end(&mut buf) {
            Ok(20) => Ok(buf),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::Other,
                "Can't read enough random bytes",
            )),
            Err(e) => Err(e),
        }
    }

    /// Set retry count and retry delay.
    ///
    /// Retry count defaults to `3`.
    /// Retry delay defaults to `200`.
    pub fn set_retry(&mut self, count: u32, delay: u32) {
        self.retry_count = count;
        self.retry_delay = delay;
    }

    pub fn lock_instance(
        &self,
        client: &redis::Client,
        resource: &[u8],
        val: &[u8],
        ttl: usize,
    ) -> bool {
        let mut con = match client.get_connection() {
            Err(_) => return false,
            Ok(val) => val,
        };
        let result: RedisResult<Value> = redis::cmd("SET")
            .arg(resource)
            .arg(val)
            .arg("nx")
            .arg("px")
            .arg(ttl)
            .query(&mut con);
        match result {
            Ok(Okay) => true,
            Ok(_) | Err(_) => false,
        }
    }

    /// Acquire the lock for the given resource and the requested TTL.
    ///
    /// If it succeeds, a `Lock` instance is returned,
    /// including the value and the validity time
    ///
    /// If it fails. `None` is returned.
    /// A user should retry after a short wait time.
    pub fn lock(&self, resource: &[u8], val: Vec<u8>, ttl: usize, retry_count: Option<u32>, retry_delay: Option<u32>) -> Option<Lock> {
        // let val = self.get_unique_lock_id().unwrap();

        let retry_count = {
            ||
                if let Some(count) = retry_count {
                    if count > 0 {
                        count
                    } else {
                        self.retry_count
                    }
                } else {
                    self.retry_count
                }
        }();

        for _ in 0..retry_count {
            let mut n = 0;
            let start_time = Instant::now();
            for client in &self.servers {
                if self.lock_instance(client, resource, &val, ttl) {
                    n += 1;
                }
            }

            let drift = (ttl as f32 * CLOCK_DRIFT_FACTOR) as usize + 2;
            let elapsed = start_time.elapsed();
            let validity_time = ttl
                .saturating_sub(drift)
                .saturating_sub(elapsed.as_secs() as usize * 1000)
                .saturating_sub(elapsed.subsec_nanos() as usize / 1_000_000);

            if n >= self.quorum && validity_time > 0 {
                return Some(Lock {
                    lock_manager: self,
                    resource: resource.to_vec(),
                    val,
                    validity_time,
                });
            } else {
                for client in &self.servers {
                    self.unlock_instance(client, resource, &val);
                }
            }

            if let Some(retry_delay) = retry_delay {
                sleep(Duration::from_millis(u64::from(retry_delay)))
            } else {
                let mut rng = thread_rng();
                let n = rng.gen_range(0..self.retry_delay);
                sleep(Duration::from_millis(u64::from(n)));
            }
        }
        None
    }

    /// Acquire the lock for the given resource and the requested TTL. \
    /// Will wait and yield current task (tokio runtime) until the lock \
    /// is acquired
    ///
    /// Returns a `RedisLockGuard` instance which is a RAII wrapper for \
    /// the old `Lock` object
    #[cfg(feature = "async")]
    pub async fn acquire_async(&self, resource: &[u8], val: Vec<u8>, ttl: usize, retry_count: Option<u32>, retry_delay: Option<u32>) -> RedisLockGuard<'_> {
        let lock;
        loop {
            match self.lock(resource, val.clone(), ttl, retry_count, retry_delay) {
                Some(l) => {
                    lock = l;
                    break;
                }
                None => tokio::task::yield_now().await,
            }
        }
        RedisLockGuard { lock }
    }

    pub fn acquire(&self, resource: &[u8], val: Vec<u8>, ttl: usize, retry_count: Option<u32>, retry_delay: Option<u32>) -> RedisLockGuard<'_> {
        let lock;
        loop {
            if let Some(l) = self.lock(resource, val.clone(), ttl, retry_count, retry_delay) {
                lock = l;
                break;
            }
        }
        RedisLockGuard { lock }
    }

    pub fn unlock_instance(&self, client: &redis::Client, resource: &[u8], val: &[u8]) -> bool {
        let mut con = match client.get_connection() {
            Err(_) => return false,
            Ok(val) => val,
        };
        let script = redis::Script::new(UNLOCK_SCRIPT);
        let result: RedisResult<i32> = script.key(resource).arg(val).invoke(&mut con);
        match result {
            Ok(val) => val == 1,
            Err(_) => false,
        }
    }

    /// Unlock the given lock.
    ///
    /// Unlock is best effort. It will simply try to contact all instances
    /// and remove the key.
    pub fn unlock(&self, lock: &Lock) {
        for client in &self.servers {
            self.unlock_instance(client, &lock.resource, &lock.val);
        }
    }
}