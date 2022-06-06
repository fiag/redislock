# redislock-rs - Distributed locks with Redis

![GitHub Workflow Status](https://img.shields.io/github/workflow/status/badboy/redlock-rs/CI)
![Crates.io](https://img.shields.io/crates/v/redlock)

This is an implementation of Redislock, the [distributed locking mechanism][distlock] built on top of Redis. It is more or
less a port of the [Ruby version][redlock.rb].

It includes a sample application in [main.rs](src/main.rs).

## Build

```
cargo build --release
```

## Usage

```rust
use redislock::{random_char, RedisLock};

fn main() {
    let rl = RedisLock::new(vec![
        "redis://127.0.0.1:6380/",
        "redis://127.0.0.1:6381/",
        "redis://127.0.0.1:6382/"]);

    let lock;
    loop {
        let val = random_char(Some(20));
        match rl.lock("mutex".as_bytes(), val, 1000, None, None) {
            Some(l) => {
                lock = l;
                break;
            }
            None => ()
        }
    }

    // Critical section

    rl.unlock(&lock);
}
```

## Tests

Run tests with:

```
cargo test
```

Run sample application with:

```
cargo run --release
```

## Contribute

If you find bugs or want to help otherwise, please [open an issue](https://github.com/hxuchen/redislock/issues).

## Maintainer

* Forked from [badboy/redlock-rs](https://github.com/badboy/redlock-rs), maintained by [@IronC](https://github.com/hxuchen)

## License

BSD. See [LICENSE](LICENSE).

[distlock]: http://redis.io/topics/distlock

[redlock.rb]: https://github.com/antirez/redlock-rb
