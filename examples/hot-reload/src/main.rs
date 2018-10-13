extern crate actix;
extern crate actix_lua;
extern crate futures;
#[macro_use]
extern crate lazy_static;

use actix::prelude::*;
use actix_lua::{LuaActor, LuaActorBuilder, LuaMessage};
use std::time::Duration;

lazy_static! {
    static ref SCRIPT_ACTOR: Addr<LuaActor> = {
        LuaActorBuilder::new()
            .on_handle_with_lua(
                r#"
            -- flush the require cache
            --
            -- in production you shouldn't flush it for every message,
            -- instead you should wait for some kind of "singal", such as file watcher
            -- to flush it only when necessary
            package.loaded["script"] = nil

            -- try to load the script, print error if failed
            local ok, ret = pcall(require, "script")
            if not ok then
                print(ret)
            end

            local handler = ret
            -- try to call the script
            local ok, ret = pcall(handler, ctx.msg)
            if not ok then
                print(ret)
            end

            return 0
            "#,
            ).build()
            .unwrap()
            .start()
    };
}

struct Tick {
    counter: i64,
}

impl Actor for Tick {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.notify_later(TickMessage {}, Duration::new(1, 0));
    }
}

struct TickMessage {}

impl Message for TickMessage {
    type Result = ();
}

impl Handler<TickMessage> for Tick {
    type Result = ();

    fn handle(&mut self, _msg: TickMessage, ctx: &mut Context<Self>) -> Self::Result {
        SCRIPT_ACTOR.do_send(LuaMessage::from(self.counter));
        self.counter += 1;
        ctx.notify_later(TickMessage {}, Duration::new(1, 0));
    }
}

fn main() {
    System::run(|| {
        Tick { counter: 0 }.start();
    });
    println!("Hello, world!");
}
