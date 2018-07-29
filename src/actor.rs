use actix::prelude::*;
use rlua::Error as LuaError;
use rlua::{FromLua, Function, Lua, Value};

use std::fs::File;
use std::io::prelude::*;

use message::LuaMessage;

const LUA_PRELUDE: &str = r#"
function notify(msg)
    _notify_rpc(msg)
end

function notify_later(msg, after)
    _notify_rpc({_rpc_type = "notify_later", msg = msg, after = after})
end
"#;

pub struct LuaActor {
    vm: Lua,
}

impl LuaActor {
    pub fn new(script: &str) -> Result<LuaActor, LuaError> {
        let vm = Lua::new();
        vm.eval::<()>(&script, Some("Init"))?;
        vm.eval::<()>(&LUA_PRELUDE, Some("Prelude"))?;

        Result::Ok(LuaActor { vm })
    }

    pub fn new_from_file(path: &str) -> Result<LuaActor, LuaError> {
        let mut f = File::open(path).expect("File not found");
        let mut body = String::new();
        f.read_to_string(&mut body).expect("Failed to read file");

        let actor = LuaActor::new(&body)?;
        Result::Ok(actor)
    }

    fn invoke_in_scope(
        &mut self,
        ctx: &mut Context<Self>,
        func_name: &str,
        arg: LuaMessage,
    ) -> <Self as Handler<LuaMessage>>::Result {
        self.vm.scope(|scope| {
            let globals = self.vm.globals();

            let notify = scope
                .create_function_mut(|_, msg| {
                    ctx.notify(msg);
                    Ok(())
                })
                .unwrap();
            globals.set("_notify_rpc", notify).unwrap();

            let lua_handle: Result<Function, LuaError> = globals.get(func_name);
            if let Ok(f) = lua_handle {
                LuaMessage::from_lua(f.call::<LuaMessage, Value>(arg).unwrap(), &self.vm).unwrap()
            } else {
                LuaMessage::Nil
            }
        })
    }
}

impl Actor for LuaActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        self.invoke_in_scope(ctx, "started", LuaMessage::Nil);
    }

    fn stopped(&mut self, ctx: &mut Context<Self>) {
        self.invoke_in_scope(ctx, "stopped", LuaMessage::Nil);
    }
}

impl Handler<LuaMessage> for LuaActor {
    type Result = LuaMessage;

    fn handle(&mut self, msg: LuaMessage, ctx: &mut Context<Self>) -> Self::Result {
        if let LuaMessage::RPCNotifyLater(msg, d) = msg {
            ctx.notify_later(*msg, d);

            LuaMessage::Nil
        } else {
            self.invoke_in_scope(ctx, "handle", msg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_timer::Delay;
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::prelude::Future;

    #[test]
    fn lua_actor() {
        let system = System::new("test");

        let lua_addr = LuaActor::new_from_file("./src/test.lua").unwrap().start();

        let l = lua_addr.send(LuaMessage::from(3));
        Arbiter::spawn(l.map(|res| {
            assert_eq!(res, LuaMessage::from(423));
            System::current().stop();
        }).map_err(|e| println!("actor dead {}", e)));

        system.run();
    }

    #[test]
    fn lua_actor_table() {
        let system = System::new("test");

        let lua_addr = LuaActor::new(
            r#"
        function handle(msg)
            return {x = 1}
        end
        "#,
        ).unwrap()
            .start();

        let l = lua_addr.send(LuaMessage::from(3));
        Arbiter::spawn(l.map(|res| {
            let mut t = HashMap::new();
            t.insert("x".to_string(), LuaMessage::from(1));

            assert_eq!(res, LuaMessage::from(t));
            System::current().stop();
        }).map_err(|e| println!("actor dead {}", e)));

        system.run();
    }

    #[test]
    fn lua_actor_started_hook_is_not_function() {
        let system = System::new("test");

        let lua_addr = LuaActor::new(
            r#"
        started = 1

        function handle(msg)
            return {x = 1}
        end
        "#,
        ).unwrap()
            .start();

        let l = lua_addr.send(LuaMessage::from(3));
        Arbiter::spawn(l.map(|res| {
            let mut t = HashMap::new();
            t.insert("x".to_string(), LuaMessage::from(1));

            assert_eq!(res, LuaMessage::from(t));
            System::current().stop();
        }).map_err(|e| println!("actor dead {}", e)));

        system.run();
    }

    #[test]
    fn lua_actor_state() {
        let system = System::new("test");

        let lua_addr = LuaActor::new(
            r#"
        state = 1

        function handle(msg)
            state = state + 1
            return state
        end
        "#,
        ).unwrap()
            .start();

        let l = lua_addr.send(LuaMessage::Nil);
        Arbiter::spawn(l.map(move |res| {
            assert_eq!(res, LuaMessage::from(2));
            let l2 = lua_addr.send(LuaMessage::Nil);
            Arbiter::spawn(l2.map(|res| {
                assert_eq!(res, LuaMessage::from(3));
                System::current().stop();
            }).map_err(|e| println!("actor dead {}", e)));
        }).map_err(|e| println!("actor dead {}", e)));

        system.run();
    }

    #[test]
    fn lua_actor_notify() {
        let system = System::new("test");

        let addr = LuaActor::new(
            r#"
        c = 0
        function started ()
            notify(100)
        end

        function handle(msg)
            print("handling", msg)
            c = c + msg
            return c
        end
        "#,
        ).unwrap()
            .start();

        let delay = Delay::new(Duration::from_secs(1)).map(move |()| {
            let l = addr.send(LuaMessage::from(1));
            Arbiter::spawn(l.map(|res| {
                assert_eq!(res, LuaMessage::from(101));
                System::current().stop();
            }).map_err(|e| println!("actor dead {}", e)))
        });
        Arbiter::spawn(delay.map_err(|e| println!("actor dead {}", e)));

        system.run();
    }

    #[test]
    fn lua_actor_notify_later() {
        let system = System::new("test");

        let addr = LuaActor::new(
            r#"
        c = 0

        function started()
            notify_later(100, 1)
        end

        function handle(msg)
            print("handling", msg)
            c = c + msg
            return c
        end
        "#,
        ).unwrap()
            .start();
        let delay = Delay::new(Duration::from_secs(2)).map(move |()| {
            let l2 = addr.send(LuaMessage::from(1));
            Arbiter::spawn(l2.map(|res| {
                assert_eq!(res, LuaMessage::from(101));
                System::current().stop();
            }).map_err(|e| println!("actor dead {}", e)))
        });
        Arbiter::spawn(delay.map_err(|e| println!("actor dead {}", e)));

        system.run();
    }
}
