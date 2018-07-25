use actix::prelude::*;
use rlua::Error as LuaError;
use rlua::{FromLua, Function, Lua, Value};

use std::fs::File;
use std::io::prelude::*;

use message::LuaMessage;

pub struct LuaActor {
    vm: Lua,
}

impl LuaActor {
    pub fn new(script: &str) -> Result<LuaActor, LuaError> {
        let vm = Lua::new();
        vm.eval::<()>(&script, Some("Init"))?;

        Result::Ok(LuaActor { vm })
    }

    pub fn new_from_file(path: &str) -> Result<LuaActor, LuaError> {
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

impl Handler<LuaMessage> for LuaActor {
    type Result = LuaMessage;

    fn handle(&mut self, msg: LuaMessage, _: &mut Context<Self>) -> Self::Result {
        let globals = self.vm.globals();
        let lua_handle: Function = globals.get("handle").unwrap();
        LuaMessage::from_lua(lua_handle.call::<LuaMessage, Value>(msg).unwrap(), &self.vm).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::prelude::Future;

    #[test]
    fn lua_actor() {
        let system = System::new("test");

        let lua_addr = LuaActor::new_from_file("./src/test.lua").unwrap().start();

        let l = lua_addr.send(LuaMessage::from(3));
        Arbiter::spawn(l.map(|res| {
            assert_eq!(res, LuaMessage::from(423));
            println!("GOT {:?}", res);
            System::current().stop();
        }).map_err(|e| println!("actor dead {}", e)));

        system.run();
    }
}
