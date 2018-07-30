# actix-lua

[![](http://meritbadge.herokuapp.com/actix-lua)](https://crates.io/crates/actix-lua)

Write [Actix](https://github.com/actix/actix) actor with [Lua](https://www.lua.org/).

## Usage

Add `actix-lua` to your `Cargo.toml`:

```toml
[dependencies]
actix-lua = "0.1"
```

#### Implement an Actor in Lua

You can handle messages by defining a `handle` function in Lua. For example:

```rust
extern crate actix_lua;
use actix_lua::{LuaActor, LuaMessage};

fn main () {
    let system = System::new("test");
    let addr = LuaActor::new(r#"
      function handle(msg)
        return msg + 42
      end
    "#).unwrap().start();

    let res = add.send(LuaMessage:from(100));
    // return: 142
}
```

## Messages

Since Lua is a dynamic typed language. We use one message type `LuaMessage` to represent all kind of types of a message Lua can send/receive.

You can convert most of the primitive types to `LuaMessage` with `LuaMessage::from()`.

## Lua API

The following function is available in the Lua script:

#### `notify(msg)`

Send message `msg` to self.

#### `notify_later(msg, seconds)`

Send message `msg` to self after specified period of time.

## License

The MIT License