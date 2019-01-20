# actix-lua

[![Build Status](https://travis-ci.org/poga/actix-lua.svg?branch=master)](https://travis-ci.org/poga/actix-lua)
[![Latest Version](https://img.shields.io/crates/v/actix-lua.svg)](https://crates.io/crates/actix-lua)
[![API Documentation](https://docs.rs/actix-lua/badge.svg)](https://docs.rs/actix-lua)

A safe scripting environment for [actix](https://github.com/actix/actix) with the [Lua Programming Language](https://www.lua.org):

* Each `LuaActor` is an isolated Lua VM.
* Communicate between actors with predefined message types: `String`, `Integer`, `Number`, `Boolean`, `Nil`, and `Table`.
* Asynchronous `send` between actors with Lua coroutine.

For more info about the "safety", check [rlua's README](https://github.com/kyren/rlua).

## Synopsis

A basic Lua actor

```rust
extern crate actix_lua;
use actix_lua::{LuaActorBuilder, LuaMessage};

fn main () {
    let addr = LuaActorBuilder::new()
        .on_handle_with_lua(r#"return ctx.msg + 42"#)
        .build()
        .unwrap()
        .start();

    let res = addr.send(LuaMessage:from(100));
    // return: 142
}
```

You can send messages to other actor asynchronously with `ctx.send`

```rust
struct Callback;
impl Actor for Callback {
    type Context = Context<Self>;
}

impl Handler<LuaMessage> for Callback {
    type Result = LuaMessage;

    fn handle(&mut self, msg: LuaMessage, _ctx: &mut Context<Self>) -> Self::Result {
        LuaMessage::String("hello".to_string())
    }
}

let mut actor = LuaActorBuilder::new()
    // create a new LuaActor from a lua script when the actor is started.
    // send message to the newly created actor with `ctx.send`, block and wait for its response.
    .on_started_with_lua(
        r#"
    local result = ctx.send("callback, "Hello")
    print(result) -- print "hello"
    "#).build()
    .unwrap();

actor.add_recipients("callback", Callback.start().recipient());

actor.start();
```

## Install

Add `actix-lua` to your `Cargo.toml`:

```toml
[dependencies]
actix-lua = "0.5"
```

## Example

Check [examples](https://github.com/poga/actix-lua/tree/master/examples) directory.

There's also a write-up about analyzing streaming data with actix-lua. [link](https://devpoga.org/post/parsing-streaming-data-actix-lua/)

## Lua Actor

Use [`LuaActor`](https://docs.rs/actix-lua/latest/actix_lua/struct.LuaActor.html) to integrate Lua scripts to your system with actor model.

### Message

In actor model, actors communicate with messages. `LuaMessage` is the only message type accepted by `LuaActor`:

* `LuaMessage` can be converted to/from primitive types with `LuaMessage::from()`.
* Lua types(e.g. number, table) will be convert to `LuaMessage` automatically.

### Lua API

**Note**: Avoid declaring global variables in your Lua script. It might conflict with future `actix-lua` update and break your program.

#### `ctx.msg`

The message sent to Lua actor.

#### `ctx.notify(msg)`

Send message `msg` to self.

#### `ctx.notify_later(msg, seconds)`

Send message `msg` to self after specified period of time.

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
