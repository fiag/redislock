extern crate core;

use anyhow::Result;
use redislock::{random_char, Lock, RedisLock};
use std::env;

// static DOCKER: Lazy<Cli> = Lazy::new(Cli::default);
// static CONTAINERS: Lazy<Vec<Container<Redis>>> = Lazy::new(|| {
//     (0..3)
//         .map(|_| DOCKER.run(Redis::default().with_tag("6-alpine")))
//         .collect()
// });
// static ADDRESSES: Lazy<Vec<String>> = Lazy::new(|| match std::env::var("ADDRESSES") {
//     Ok(addresses) => addresses.split(',').map(String::from).collect(),
//     Err(_) => CONTAINERS
//         .iter()
//         .map(|c| format!("redis://localhost:{}", c.get_host_port_ipv4(6379)))
//         .collect(),
// });

fn redis_address() -> Vec<String> {
    match env::var("ADDRESSES") {
        Ok(addresses) => addresses.split(',').map(String::from).collect(),
        Err(_) => panic!("redis ADDRESSES must be exported"),
    }
}

#[test]
fn test_redlock_get_unique_id() -> Result<()> {
    let rl = RedisLock::new(Vec::<String>::new());
    assert_eq!(rl.get_unique_lock_id()?.len(), 20);
    Ok(())
}

#[test]
fn test_redlock_get_unique_id_uniqueness() -> Result<()> {
    let rl = RedisLock::new(Vec::<String>::new());

    let id1 = rl.get_unique_lock_id()?;
    let id2 = rl.get_unique_lock_id()?;

    assert_eq!(20, id1.len());
    assert_eq!(20, id2.len());
    assert_ne!(id1, id2);
    Ok(())
}

#[test]
fn test_redlock_valid_instance() {
    println!("{}", redis_address().join(","));
    let rl = RedisLock::new(redis_address().clone());
    assert_eq!(3, rl.servers.len());
    assert_eq!(2, rl.quorum());
}

#[test]
fn test_redlock_direct_unlock_fails() -> Result<()> {
    println!("{}", redis_address().join(","));
    let rl = RedisLock::new(redis_address().clone());
    let key = rl.get_unique_lock_id()?;

    let val = rl.get_unique_lock_id()?;
    assert!(!rl.unlock_instance(&rl.servers[0], &key, &val));
    Ok(())
}

#[test]
fn test_redlock_direct_unlock_succeeds() -> Result<()> {
    println!("{}", redis_address().join(","));
    let rl = RedisLock::new(redis_address().clone());
    let key = rl.get_unique_lock_id()?;

    let val = rl.get_unique_lock_id()?;
    let mut con = rl.servers[0].get_connection()?;
    redis::cmd("SET").arg(&*key).arg(&*val).execute(&mut con);

    assert!(rl.unlock_instance(&rl.servers[0], &key, &val));
    Ok(())
}

#[test]
fn test_redlock_direct_lock_succeeds() -> Result<()> {
    println!("{}", redis_address().join(","));
    let rl = RedisLock::new(redis_address().clone());
    let key = rl.get_unique_lock_id()?;

    let val = random_char(Some(20));
    let mut con = rl.servers[0].get_connection()?;

    redis::cmd("DEL").arg(&*key).execute(&mut con);
    assert!(rl.lock_instance(&rl.servers[0], &*key, &*val, 1000));
    Ok(())
}

#[test]
fn test_redlock_unlock() -> Result<()> {
    println!("{}", redis_address().join(","));
    let rl = RedisLock::new(redis_address().clone());
    let key = rl.get_unique_lock_id()?;

    let val = rl.get_unique_lock_id()?;
    let mut con = rl.servers[0].get_connection()?;
    let _: () = redis::cmd("SET")
        .arg(&*key)
        .arg(&*val)
        .query(&mut con)
        .unwrap();

    let lock = Lock {
        lock_manager: &rl,
        resource: key,
        val,
        validity_time: 0,
    };
    rl.unlock(&lock);
    Ok(())
}

#[test]
fn test_redlock_lock() -> Result<()> {
    println!("{}", redis_address().join(","));
    let rl = RedisLock::new(redis_address().clone());

    let key = rl.get_unique_lock_id()?;
    let val = random_char(Some(20));

    match rl.lock(&key, val, 1000, None, None) {
        Some(lock) => {
            assert_eq!(key, lock.resource);
            assert_eq!(20, lock.val.len());
            assert!(lock.validity_time > 900);
            assert!(
                lock.validity_time > 900,
                "validity time: {}",
                lock.validity_time
            );
        }
        None => panic!("Lock failed"),
    };
    Ok(())
}

#[test]
fn test_redlock_lock_unlock() -> Result<()> {
    println!("{}", redis_address().join(","));
    let rl = RedisLock::new(redis_address().clone());
    let rl2 = RedisLock::new(redis_address().clone());

    let key = rl.get_unique_lock_id()?;
    let val1 = random_char(Some(20));
    let val2 = random_char(Some(20));

    let lock = rl.lock(&key, val1.clone(), 1000, None, None).unwrap();
    assert!(
        lock.validity_time > 900,
        "validity time: {}",
        lock.validity_time
    );

    if let Some(_l) = rl2.lock(&key, val2.clone(), 1000, None, None) {
        panic!("Lock acquired, even though it should be locked")
    }

    rl.unlock(&lock);

    match rl2.lock(&key, val2, 1000, None, None) {
        Some(l) => assert!(l.validity_time > 900),
        None => panic!("Lock couldn't be acquired"),
    }
    Ok(())
}

#[test]
fn test_redlock_lock_unlock_raii() -> Result<()> {
    println!("{}", redis_address().join(","));
    let rl = RedisLock::new(redis_address().clone());
    let rl2 = RedisLock::new(redis_address().clone());

    let key = rl.get_unique_lock_id()?;
    let val1 = random_char(Some(20));
    let val2 = random_char(Some(20));
    {
        let lock_guard = rl.acquire(&key, val1.clone(), 1000, None, None);
        let lock = &lock_guard.lock;
        assert!(
            lock.validity_time > 900,
            "validity time: {}",
            lock.validity_time
        );

        if let Some(_l) = rl2.lock(&key, val2.clone(), 1000, None, None) {
            panic!("Lock acquired, even though it should be locked")
        }
    }

    match rl2.lock(&key, val2.clone(), 1000, None, None) {
        Some(l) => assert!(l.validity_time > 900),
        None => panic!("Lock couldn't be acquired"),
    }
    Ok(())
}
