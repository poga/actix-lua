extern crate actix;
extern crate actix_lua;
extern crate futures;
extern crate rlua;
extern crate tokio;

use actix::dev::{MessageResponse, ResponseChannel};
use actix::prelude::*;
use actix_lua::test;
use futures::future::Future;
use rlua::Error as LuaError;
use rlua::Result as LuaResult;
use rlua::{FromLua, Function, Lua, ToLua, Value};

use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;

struct LuaActor {
    vm: Lua,
}

impl LuaActor {
    fn new(script: &str) -> Result<LuaActor, LuaError> {
        let vm = Lua::new();
        vm.eval::<()>(&script, Some("Init"))?;

        Result::Ok(LuaActor { vm: vm })
    }

    fn new_from_file(path: &str) -> Result<LuaActor, LuaError> {
        let mut f = File::open(path).expect("File not found");
        let mut body = String::new();
        f.read_to_string(&mut body).expect("Failed to read file");

        let actor = LuaActor::new(&body)?;
        Result::Ok(actor)
    }
}

impl Actor for LuaActor {
    type Context = Context<Self>;

    fn started(&mut self, _: &mut Context<Self>) {
        let globals = self.vm.globals();
        let lua_handle: Function = globals.get("started").unwrap();
        lua_handle.call::<(), ()>(()).unwrap()
    }

    fn stopped(&mut self, _: &mut Context<Self>) {
        let globals = self.vm.globals();
        let lua_handle: Function = globals.get("stopped").unwrap();
        lua_handle.call::<(), ()>(()).unwrap()
    }
}

#[derive(Debug)]
enum LuaMessage {
    String(String),
    Integer(i64),
    Number(f64),
    Boolean(bool),
    Nil,
}

impl<A, M> MessageResponse<A, M> for LuaMessage
where
    A: Actor,
    M: Message<Result = LuaMessage>,
{
    fn handle<R: ResponseChannel<M>>(self, _: &mut A::Context, tx: Option<R>) {
        if let Some(tx) = tx {
            tx.send(self);
        }
    }
}

impl Message for LuaMessage {
    type Result = LuaMessage;
}

impl<'l> From<&'l str> for LuaMessage {
    fn from(s: &'l str) -> Self {
        LuaMessage::String(s.to_string())
    }
}

impl<'lua> FromLua<'lua> for LuaMessage {
    fn from_lua(v: Value, lua: &'lua Lua) -> LuaResult<LuaMessage> {
        match v {
            Value::Integer(x) => Ok(LuaMessage::Integer(lua.coerce_integer(v)? as i64)),
            Value::String(x) => Ok(LuaMessage::String(String::from_lua(Value::String(x), lua)?)),
            _ => unimplemented!(),
        }
    }
}

impl<'lua> ToLua<'lua> for LuaMessage {
    fn to_lua(self, lua: &'lua Lua) -> LuaResult<Value<'lua>> {
        match self {
            LuaMessage::String(x) => Ok(Value::String(lua.create_string(&x)?)),
            LuaMessage::Integer(x) => Ok(Value::Integer(x)),
            _ => unimplemented!(),
        }
    }
}

impl Handler<LuaMessage> for LuaActor {
    type Result = LuaMessage;

    fn handle(&mut self, msg: LuaMessage, _: &mut Context<Self>) -> Self::Result {
        let globals = self.vm.globals();
        let lua_handle: Function = globals.get("handle").unwrap();
        LuaMessage::from_lua(lua_handle.call::<LuaMessage, Value>(msg).unwrap(), &self.vm).unwrap()
    }
}

pub fn main() {
    test();

    let system = System::new("test");

    let lua_addr = LuaActor::new_from_file("./src/test.lua").unwrap().start();

    let l = lua_addr.send(LuaMessage::from("foo"));
    Arbiter::spawn(
        l.map(|res| println!("GOT {:?}", res))
            .map_err(|e| println!("actor dead {}", e)),
    );

    system.run();
}
