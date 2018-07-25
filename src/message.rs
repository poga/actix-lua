extern crate actix;
extern crate rlua;

use actix::dev::{MessageResponse, ResponseChannel};
use actix::prelude::*;
use rlua::Result as LuaResult;
use rlua::{FromLua, Lua, ToLua, Value};

#[derive(Debug)]
pub enum LuaMessage {
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

impl From<bool> for LuaMessage {
    fn from(s: bool) -> Self {
        LuaMessage::Boolean(s)
    }
}

impl<'l> From<&'l str> for LuaMessage {
    fn from(s: &'l str) -> Self {
        LuaMessage::String(s.to_string())
    }
}

macro_rules! lua_message_convert_int {
    ($x:ty) => {
        impl From<$x> for LuaMessage {
            fn from(s: $x) -> Self {
                LuaMessage::Integer(s as i64)
            }
        }
    };
}

lua_message_convert_int!(i8);
lua_message_convert_int!(u8);
lua_message_convert_int!(i16);
lua_message_convert_int!(u16);
lua_message_convert_int!(i32);
lua_message_convert_int!(u32);
lua_message_convert_int!(i64);
lua_message_convert_int!(isize);
lua_message_convert_int!(usize);

macro_rules! lua_message_convert_float {
    ($x:ty) => {
        impl From<$x> for LuaMessage {
            fn from(s: $x) -> Self {
                LuaMessage::Number(s as f64)
            }
        }
    };
}

lua_message_convert_float!(f32);
lua_message_convert_float!(f64);

impl<'lua> FromLua<'lua> for LuaMessage {
    fn from_lua(v: Value, lua: &'lua Lua) -> LuaResult<LuaMessage> {
        match v {
            Value::String(x) => Ok(LuaMessage::String(String::from_lua(Value::String(x), lua)?)),
            Value::Integer(_) => Ok(LuaMessage::Integer(lua.coerce_integer(v)? as i64)),
            Value::Number(_) => Ok(LuaMessage::Number(lua.coerce_number(v)? as f64)),
            Value::Boolean(b) => Ok(LuaMessage::Boolean(b)),
            Value::Nil => Ok(LuaMessage::Nil),

            _ => unimplemented!(),
        }
    }
}

impl<'lua> ToLua<'lua> for LuaMessage {
    fn to_lua(self, lua: &'lua Lua) -> LuaResult<Value<'lua>> {
        match self {
            LuaMessage::String(x) => Ok(Value::String(lua.create_string(&x)?)),
            LuaMessage::Integer(x) => Ok(Value::Integer(x)),
            LuaMessage::Number(x) => Ok(Value::Number(x)),
            LuaMessage::Boolean(x) => Ok(Value::Boolean(x)),
            LuaMessage::Nil => Ok(Value::Nil),
        }
    }
}
