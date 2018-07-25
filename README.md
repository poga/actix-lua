# Actix-Lua

[![](http://meritbadge.herokuapp.com/actix-lua)](https://crates.io/crates/actix-lua)

[Actix](https://github.com/actix/actix) actor with [Lua](https://www.lua.org/).

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

## Message Type

`LuaActor` only accept messages with type `LuaMessage`. The result of `LuaMessage` is also `LuaMessage`.

`LuaMessage` is defined as:

```rust
pub enum LuaMessage {
    String(String),
    Integer(i64),
    Number(f64),
    Boolean(bool),
    Nil,
}
```

It's the sender's job to check the returned value type from Lua is what they want.

## License

The MIT License