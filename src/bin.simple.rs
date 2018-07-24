extern crate actix;
extern crate actix_lua;
extern crate futures;
extern crate rlua;
extern crate tokio;

use actix::dev::{MessageResponse, ResponseChannel};
use actix::prelude::*;
use actix_lua::test;
use futures::future::Future;
use rlua::{Function, Lua, Table, ToLua, Value};
use tokio::timer::Delay;

use std::collections::HashMap;
use std::time::{Duration, Instant};

enum Messages {
    Ping,
    Pong,
}

impl Message for Messages {
    type Result = Responses;
}

#[derive(Debug)]
enum Responses {
    GotPing,
    GotPong,
}

impl<A, M> MessageResponse<A, M> for Responses
where
    A: Actor,
    M: Message<Result = Responses>,
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
        if let Some(tx) = tx {
            tx.send(self);
        }
    }
}

struct MyActor {
    count: usize,
}

impl Actor for MyActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        println!("actor started");
    }

    fn stopped(&mut self, ctx: &mut Context<Self>) {
        println!("actor stopped");
    }
}

impl Handler<Messages> for MyActor {
    type Result = Responses;

    fn handle(&mut self, msg: Messages, ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            Messages::Ping => Responses::GotPing,
            Messages::Pong => Responses::GotPong,
        }
    }
}

impl Handler<Ping> for MyActor {
    type Result = usize;

    fn handle(&mut self, msg: Ping, ctx: &mut Context<Self>) -> Self::Result {
        println!("received {}", msg.0);
        self.count += msg.0;
        self.count
    }
}

struct LuaActor {
    vm: Lua,
}

impl Actor for LuaActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        let globals = self.vm.globals();
        let luaHandle: Function = globals.get("started").unwrap();
        luaHandle.call::<(), ()>(()).unwrap()
    }

    fn stopped(&mut self, ctx: &mut Context<Self>) {
        let globals = self.vm.globals();
        let luaHandle: Function = globals.get("stopped").unwrap();
        luaHandle.call::<(), ()>(()).unwrap()
    }
}

enum LuaMessage {
    StringMessage(String),
    u32Message(u32),
}

impl Message for LuaMessage {
    type Result = LuaResponse;
}

#[derive(Debug)]
struct LuaResponse {}

impl From<String> for LuaMessage {
    fn from(s: String) -> Self {
        LuaMessage::StringMessage(s)
    }
}

impl<A, M> MessageResponse<A, M> for LuaResponse
where
    A: Actor,
    M: Message<Result = LuaResponse>,
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
        if let Some(tx) = tx {
            tx.send(self);
        }
    }
}

trait LuaMessageTrait {
    type Result;
}

impl Handler<LuaMessage> for LuaActor {
    type Result = LuaResponse;

    fn handle(&mut self, msg: LuaMessage, ctx: &mut Context<Self>) -> Self::Result {
        let globals = self.vm.globals();
        let lua_handle: Function = globals.get("handle").unwrap();
        match msg {
            LuaMessage::StringMessage(s) => {
                let res = lua_handle.call::<_, Table>(s).unwrap();
                println!("{:?}", res.get::<String, _>("x".to_string()).unwrap());
            }
            LuaMessage::u32Message(u) => {
                lua_handle.call::<_, Table>(u).unwrap();
            }
        };

        LuaResponse {}
    }
}

struct Ping(usize);

impl Message for Ping {
    type Result = usize;
}

trait ToLuaMessage<'lua> {
    type LuaType: ToLua<'lua>;

    fn to_lua(&mut self) -> Self::LuaType;
}

impl<'lua> ToLuaMessage<'lua> for Ping {
    type LuaType = usize;

    fn to_lua(&mut self) -> usize {
        self.0
    }
}

pub fn main() {
    test();

    let system = System::new("test");
    let addr = MyActor { count: 10 }.start();

    let vm = Lua::new();
    vm.eval::<()>(
        r#"
        function handle(msg)
            return {x = msg.."!"}
        end
    "#,
        None,
    );
    vm.eval::<()>(
        r#"
        function started()
            print("started lua actor")
        end
    "#,
        None,
    );
    let lua_addr = LuaActor { vm: vm }.start();

    let ping_future = addr.send(Messages::Ping);
    let pong_future = addr.send(Messages::Pong);

    let l = lua_addr.send(LuaMessages::from("foo".to_string()));
    Arbiter::spawn(
        l.map(|res| println!("GOT {:?}", res))
            .map_err(|e| println!("actor dead {}", e)),
    );

    Arbiter::spawn(
        pong_future
            .map(|res| println!("GOT {:?}", res))
            .map_err(|e| println!("actor dead {}", e)),
    );
    Arbiter::spawn(
        ping_future
            .map(|res| println!("GOT {:?}", res))
            .map_err(|e| println!("actor dead {}", e)),
    );

    system.run();
}
