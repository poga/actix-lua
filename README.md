# actix-lua

[![Build Status](https://travis-ci.org/poga/actix-lua.svg?branch=master)](https://travis-ci.org/poga/actix-lua)
[![Latest Version](https://img.shields.io/crates/v/rlua.svg)](https://crates.io/crates/rlua)
[![API Documentation](https://docs.rs/actix-lua/badge.svg)](https://docs.rs/actix-lua)

A safe scripting environment for [actix](https://github.com/actix/actix) with the [Lua Programming Language](https;//www.lua.org).

## Synopsis

```rust
extern crate actix_lua;
use actix_lua::{LuaActorBuilder, LuaMessage};

fn main () {
    let addr = LuaActorBuilder::new()
        .on_handle_with_lua(r#"return ctx.msg + 42"#)
        .build()
        .unwrap()
        .start();

    let res = add.send(LuaMessage:from(100));
    // return: 142
}
```

## Install

Add `actix-lua` to your `Cargo.toml`:

```toml
[dependencies]
actix-lua = "0.3"
```

## Message

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