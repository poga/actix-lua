use actix::dev::{MessageResponse, ResponseChannel};
use actix::prelude::*;
use rlua::Result as LuaResult;
use rlua::{FromLua, Lua, ToLua, Value};

use std::collections::HashMap;

#[derive(Debug, PartialEq, Clone)]
pub enum LuaMessage {
    String(String),
    Integer(i64),
    Number(f64),
    Boolean(bool),
    Nil,
    Table(HashMap<String, LuaMessage>),
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

impl From<String> for LuaMessage {
    fn from(s: String) -> Self {
        LuaMessage::String(s)
    }
}

macro_rules! lua_message_convert_int {
    ($x:ty) => {
        impl From<$x> for LuaMessage {
            fn from(s: $x) -> Self {
                LuaMessage::Integer(i64::from(s))
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

impl From<usize> for LuaMessage {
    fn from(s: usize) -> Self {
        LuaMessage::Integer(s as i64)
    }
}

impl From<isize> for LuaMessage {
    fn from(s: isize) -> Self {
        LuaMessage::Integer(s as i64)
    }
}

impl From<HashMap<String, LuaMessage>> for LuaMessage {
    fn from(s: HashMap<String, LuaMessage>) -> Self {
        LuaMessage::Table(s)
    }
}

macro_rules! lua_message_convert_float {
    ($x:ty) => {
        impl From<$x> for LuaMessage {
            fn from(s: $x) -> Self {
                LuaMessage::Number(f64::from(s))
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
            Value::Table(t) => Ok(LuaMessage::Table(HashMap::from_lua(Value::Table(t), lua)?)),

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
            LuaMessage::Table(x) => Ok(Value::Table(lua.create_table_from(x)?)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::discriminant;

    #[test]
    fn constructors() {
        assert_eq!(LuaMessage::from(42), LuaMessage::Integer(42));
        assert_eq!(LuaMessage::from(0), LuaMessage::Integer(0));
        assert_eq!(
            LuaMessage::from("foo"),
            LuaMessage::String("foo".to_string())
        );
        assert_eq!(LuaMessage::from(42.5), LuaMessage::Number(42.5));
        assert_eq!(LuaMessage::from(true), LuaMessage::Boolean(true));

        let mut t = HashMap::new();
        t.insert("bar".to_string(), LuaMessage::from("abc"));
        let mut t2 = HashMap::new();
        t2.insert("bar".to_string(), LuaMessage::from("abc"));
        assert_eq!(LuaMessage::from(t), LuaMessage::Table(t2));
    }

    #[test]
    fn to_lua() {
        // we only check if they have the correct variant
        let lua = Lua::new();
        assert_eq!(
            discriminant(&LuaMessage::Integer(42).to_lua(&lua).unwrap()),
            discriminant(&Value::Integer(42))
        );
        assert_eq!(
            discriminant(&LuaMessage::String("foo".to_string()).to_lua(&lua).unwrap()),
            discriminant(&Value::String(lua.create_string("foo").unwrap()))
        );
        assert_eq!(
            discriminant(&LuaMessage::Number(42.5).to_lua(&lua).unwrap()),
            discriminant(&Value::Number(42.5))
        );
        assert_eq!(
            discriminant(&LuaMessage::Boolean(true).to_lua(&lua).unwrap()),
            discriminant(&Value::Boolean(true))
        );
        assert_eq!(
            discriminant(&LuaMessage::Nil.to_lua(&lua).unwrap()),
            discriminant(&Value::Nil)
        );

        let mut t = HashMap::new();
        t.insert("bar".to_string(), LuaMessage::from("abc"));
        assert_eq!(
            discriminant(&LuaMessage::Table(t).to_lua(&lua).unwrap()),
            discriminant(&Value::Table(lua.create_table().unwrap()))
        );
    }

    #[test]
    fn from_lua() {
        // we only check if they have the correct variant
        let lua = Lua::new();
        assert_eq!(
            discriminant(&LuaMessage::from_lua(Value::Integer(42), &lua).unwrap()),
            discriminant(&LuaMessage::Integer(42))
        );
        assert_eq!(
            discriminant(&LuaMessage::from_lua(Value::Number(42.5), &lua).unwrap()),
            discriminant(&LuaMessage::Number(42.5))
        );
        assert_eq!(
            discriminant(&LuaMessage::from_lua(
                Value::String(lua.create_string("foo").unwrap()),
                &lua
            ).unwrap()),
            discriminant(&LuaMessage::String("foo".to_string()))
        );
        assert_eq!(
            discriminant(&LuaMessage::from_lua(Value::Boolean(true), &lua).unwrap()),
            discriminant(&LuaMessage::Boolean(true))
        );
        assert_eq!(
            discriminant(&LuaMessage::from_lua(Value::Nil, &lua).unwrap()),
            discriminant(&LuaMessage::Nil)
        );

        let mut t = HashMap::new();
        t.insert("bar".to_string(), LuaMessage::from("abc"));
        assert_eq!(
            discriminant(
                &LuaMessage::from_lua(Value::Table(lua.create_table().unwrap()), &lua).unwrap()
            ),
            discriminant(&LuaMessage::Table(t))
        );
    }
}
