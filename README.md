# actix-lua

[![](http://meritbadge.herokuapp.com/actix-lua)](https://crates.io/crates/actix-lua)

Write [Actix](https://github.com/actix/actix) actor with [Lua](https://www.lua.org/).

## Usage

Add `actix-lua` to your `Cargo.toml`:

```toml
[dependencies]
actix-lua = "0.2"
```

#### Build a Lua Actor

```rust
extern crate actix_lua;
use actix_lua::{LuaActorBuilder, LuaMessage};

fn main () {
    let system = System::new("test");
    let addr = LuaActorBuilder::new()
        .on_handle_with_lua(r#"return ctx.msg + 42"#)
        .build()
        .unwrap()
        .start()

    let res = add.send(LuaMessage:from(100));
    // return: 142
}
```

## Messages

Lua is a dynamic typed language. We use one message type `LuaMessage` to represent all kind of types of a message Lua can send/receive.

You can convert most of the primitive types to `LuaMessage` with `LuaMessage::from()`.

## Lua API

#### `ctx.msg`

The message sent to Lua actor.

#### `ctx.notify(msg)`

Send message `msg` to self.

#### `ctx.notify_later(msg, seconds)`

Send message `msg` to self after specified period of time.

#### `local recipient = ctx.new_actor(script_path, [actor_name])`

Create a new actor with given lua script. returns a recipient which can be used in `ctx.send` and `ctx.do_send`.

#### `local result = ctx.send(recipient, msg)`

Send message `msg` to `recipient asynchronously and wait for response.

Equivalent to `actix::Recipient.send`.

#### `ctx.do_send(recipient, msg)`

Send message `msg` to `recipient`.

Equivalent to `actix::Recipient.do_send`.

#### `ctx.terminate()`

Terminate actor execution.

## License

The MIT License