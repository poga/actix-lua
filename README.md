# Actix-Lua

[![](http://meritbadge.herokuapp.com/actix-lua)](https://crates.io/crates/actix-lua)

Write [Actix](https://github.com/actix/actix) actor with [Lua](https://www.lua.org/).

## Usage

Add `actix-lua` to your `Cargo.toml`:

```toml
[dependencies]
actix-lua = "0.1"
```

#### Implement an Actor

You can define an actor with Lua, for example:

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

    let res = add.send(LuaMessage:from(123));
}
```

## Message

Since Lua is a dynamic typed language. We use one message type `LuaMessage` to represent all types of message Lua can send/receive.

## License

The MIT License