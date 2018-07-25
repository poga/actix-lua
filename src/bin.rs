extern crate actix;
extern crate actix_lua;
extern crate rlua;
extern crate tokio;

use actix::prelude::*;
use tokio::prelude::Future;

use actix_lua::actor::LuaActor;
use actix_lua::message::LuaMessage;

pub fn main() {
    let system = System::new("test");

    let lua_addr = LuaActor::new_from_file("./src/test.lua").unwrap().start();

    let l = lua_addr.send(LuaMessage::from(3));
    Arbiter::spawn(
        l.map(|res| println!("GOT {:?}", res))
            .map_err(|e| println!("actor dead {}", e)),
    );

    system.run();
}
