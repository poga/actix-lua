use actix::prelude::*;
use rlua::Error as LuaError;
use rlua::{FromLua, Function, Lua, Value};

use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;

use message::LuaMessage;

const LUA_PRELUDE: &str = r#"
_threads = {}

_thread_id_seq = 0

function notify(msg)
    _notify_rpc(msg)
end

function notify_later(msg, after)
    _notify_rpc({_rpc_type = "notify_later", msg = msg, after = after})
end

function new_actor(name, path)
    _notify_rpc({_rpc_type = "new_lua_actor", path = path})
    return coroutine.yield()
end

function send(recipient, msg)
    _notify_rpc({_rpc_type = "send", recipient = recipient, msg = msg})
    return coroutine.yield()
end

function do_send(recipient, msg)
    _notify_rpc({_rpc_type = "do_send", recipient = recipient, msg = msg})
end

function _wrapped_handle(msg, threadID)
    if handle == nil then
        return nil
    end
    local thread
    if threadID == nil then
        thread = coroutine.create(handle)
        _threads[_thread_id_seq] = thread
        _thread_id_seq = _thread_id_seq + 1
    else
        thread = _threads[threadID]
    end

    local ok, ret = coroutine.resume(thread, msg)
    return ret
end
"#;

pub struct LuaActor {
    vm: Lua,
    recipients: HashMap<String, Recipient<LuaMessage>>,
    recipient_id_seq: u64,
}

impl LuaActor {
    pub fn new(script: &str) -> Result<LuaActor, LuaError> {
        let vm = Lua::new();
        vm.eval::<()>(&script, Some("Init"))?;
        vm.eval::<()>(&LUA_PRELUDE, Some("Prelude"))?;

        Result::Ok(LuaActor {
            vm,
            recipients: HashMap::new(),
            recipient_id_seq: 0,
        })
    }

    pub fn new_from_file(path: &str) -> Result<LuaActor, LuaError> {
        let mut f = File::open(path).expect("File not found");
        let mut body = String::new();
        f.read_to_string(&mut body).expect("Failed to read file");

        let actor = LuaActor::new(&body)?;
        Result::Ok(actor)
    }

    pub fn add_recipient(&mut self, rec: Recipient<LuaMessage>) -> Option<Recipient<LuaMessage>> {
        self.recipient_id_seq += 1;
        self.recipients
            .insert(format!("{}", self.recipient_id_seq), rec)
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

    fn invoke_in_scope_2(
        &mut self,
        ctx: &mut Context<Self>,
        func_name: &str,
        args: (LuaMessage, LuaMessage),
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
                LuaMessage::from_lua(
                    f.call::<(LuaMessage, LuaMessage), Value>(args).unwrap(),
                    &self.vm,
                ).unwrap()
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
        match msg {
            LuaMessage::RPCNotifyLater(msg, d) => {
                ctx.notify_later(*msg, d);

                LuaMessage::Nil
            }
            LuaMessage::RPCNewLuaActor(path) => {
                // save lua thread id
                let name = format!("LuaActor-{}-{}", self.recipient_id_seq, path);
                let addr = LuaActor::new_from_file(&path).unwrap().start();
                self.recipients.insert(name.clone(), addr.recipient());

                // return recipient ID to lua,
                self.invoke_in_scope_2(
                    ctx,
                    "_wrapped_handle",
                    (LuaMessage::from(name.clone()), LuaMessage::from(0)),
                )
            }

            _ => self.invoke_in_scope(ctx, "_wrapped_handle", msg),
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

    #[test]
    fn lua_actor_rpc_new_actor() {
        let system = System::new("test");

        let addr = LuaActor::new(
            r#"
        function handle(msg)
            if msg == 0 then
                return msg + 1
            end
            local id = new_actor("child", "src/test.lua")
            print("resumed", id)
            -- since this is an async coroutine, return is a no-op.
            return id
        end
        "#,
        ).unwrap()
            .start();
        let l = addr.send(LuaMessage::Nil);
        Arbiter::spawn(l.map(move |res| {
            // since the handler yield, we won't get anything in return
            assert_eq!(res, LuaMessage::Nil);

            // coroutine should still works normally
            let l2 = addr.send(LuaMessage::from(0));
            Arbiter::spawn(l2.map(|res| {
                assert_eq!(res, LuaMessage::from(1));
                System::current().stop();
            }).map_err(|e| println!("actor dead {}", e)));
        }).map_err(|e| println!("actor dead {}", e)));

        system.run();
    }

    #[test]
    fn lua_actor_do_send() {
        let system = System::new("test");

        let addr = LuaActor::new(
            r#"
        function handle(msg)
            if msg == 0 then
                return msg + 1
            end
            local id = new_actor("child", "src/test.lua")
            print("resumed", id)
            -- since this is an async coroutine, return is a no-op.
            return id
        end
        "#,
        ).unwrap()
            .start();
        let l = addr.send(LuaMessage::Nil);
        Arbiter::spawn(l.map(move |res| {
            // since the handler yield, we won't get anything in return
            assert_eq!(res, LuaMessage::Nil);

            // coroutine should still works normally
            let l2 = addr.send(LuaMessage::from(0));
            Arbiter::spawn(l2.map(|res| {
                assert_eq!(res, LuaMessage::from(1));
                System::current().stop();
            }).map_err(|e| println!("actor dead {}", e)));
        }).map_err(|e| println!("actor dead {}", e)));

        system.run();
    }
}
