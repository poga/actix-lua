use actix::prelude::*;
use rlua::Error as LuaError;
use rlua::{FromLua, Function, Lua, MultiValue, ToLua, Value};

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;
use std::rc::Rc;
use std::time::Duration;
use uuid::Uuid;

use message::LuaMessage;

const LUA_PRELUDE: &str = r#"
__threads = {}
__thread_id_seq = 0
__states = {}

-- create a new coroutine from given script
function __run(script, msg)
    -- create a new env
    local env = {}
    for k, v in pairs(_G) do
        env[k] = v
    end
    env.thread_id = __thread_id_seq
    __thread_id_seq = __thread_id_seq + 1

    local ctx = {}
    ctx.notify = notify
    ctx.notify_later = notify_later
    ctx.send = send
    ctx.do_send = do_send
    ctx.new_actor = new_actor
    ctx.msg = msg
    ctx.state = __states[script]

    env.ctx = ctx

    local f = load(script, name, "bt", env)
    local thread = coroutine.create(f)

    local ok, ret = coroutine.resume(thread)
    -- save the thread and its context if the thread yielded
    if coroutine.status(thread) == "suspended" then
        __threads[env.thread_id] = { thread = thread, ctx = ctx }
    end
    if ctx.state ~= nil then
        __states[script] = ctx.state
    end
    return ret
end

-- resume a existing coroutine
function __resume(thread_id, args)
    local thread = __threads[thread_id]
    local ok, ret = coroutine.resume(thread, args)
    if coroutine.status(thread) == "dead" then
        __threads[env.thread_id] = nil
    end
    return ret
end

"#;

pub struct LuaActor {
    vm: Lua,
    recipients: HashMap<String, Recipient<LuaMessage>>,
    handle_script: LuaMessage,
}

impl LuaActor {
    pub fn new(script: &str) -> Result<LuaActor, LuaError> {
        let vm = Lua::new();
        // vm.eval::<()>(&script, Some("Init"))?;
        vm.eval::<()>(&LUA_PRELUDE, Some("Prelude"))?;

        Result::Ok(LuaActor {
            vm,
            recipients: HashMap::new(),
            handle_script: LuaMessage::from(script.to_string()),
        })
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
        msg: LuaMessage,
    ) -> <Self as Handler<LuaMessage>>::Result {
        invoke(
            ctx,
            &mut self.vm,
            &mut self.recipients,
            func_name,
            vec![msg],
        );
        // `ctx` is used in multiple closure in the lua scope.
        // to create multiple borrow in closures, we use RefCell to move the borrow-checking to runtime.
        // Voliating the check will result in panic. Which shouldn't happend(I think) since lua is single-threaded.
        let ctx = RefCell::new(ctx);
        // We can't create a function with references to `self` and is 'static since `self` already owns Lua.
        // A function within Lua owning `self` creates self-borrowing cycle.
        // Also, Lua requires all values passed to it is 'static because we can't know when will Lua GC our value.
        // Therefore, we use scope to make sure the `__rpc` function is temporary and don't have to deal with 'static lifetime.
        //
        // (Quote from: https://github.com/kyren/rlua/issues/56#issuecomment-363928738
        // When the scope ends, the Lua function is 100% guaranteed (afaict!) to be "invalidated".
        // This means that calling the function will cause an immediate Lua error with a message like "error, call of invalidated function".)
        //
        // for reference, check https://github.com/kyren/rlua/issues/73#issuecomment-370222198
        // self.vm.scope(|scope| {
        //     let globals = self.vm.globals();

        //     let notify = scope
        //         .create_function_mut(|_, msg| {
        //             let mut ctx = ctx.borrow_mut();
        //             ctx.notify(msg);
        //             Ok(())
        //         })
        //         .unwrap();
        //     globals.set("notify", notify).unwrap();
        //     let notify_later = scope
        //         .create_function_mut(|_, (msg, secs)| {
        //             let mut ctx = ctx.borrow_mut();
        //             ctx.notify_later(msg, Duration::new(secs, 0));
        //             Ok(())
        //         })
        //         .unwrap();
        //     globals.set("notify_later", notify_later).unwrap();
        //     let new_actor =
        //         scope.create_function_mut(|_, (script_path, cb_thread_id): (String, u64)| {
        //             let recipient_id = Uuid::new_v4();
        //             let name = format!("LuaActor-{}-{}", recipient_id, &script_path);

        //             let addr = LuaActor::new_from_file(&script_path).unwrap().start();
        //             // TODO: fix this line
        //             // rec.insert(name.clone(), addr.recipient());
        //             // can't access self.vm.globals() here, use eval instead
        //             self.vm
        //                 .eval::<()>(
        //                     &format!(r#"__resume({}, "{}")"#, cb_thread_id, name),
        //                     Some("new_actor_callback"),
        //                 )
        //                 .unwrap();
        //             Ok(())
        //         });

        //     let lua_handle: Result<Function, LuaError> = globals.get(func_name);
        //     if let Ok(f) = lua_handle {
        //         LuaMessage::from_lua(f.call::<LuaMessage, Value>(arg).unwrap(), &self.vm).unwrap()
        //     } else {
        //         LuaMessage::Nil
        //     }
        // })
        LuaMessage::Nil
    }

    fn invoke_in_scope_2(
        &mut self,
        ctx: &mut Context<Self>,
        func_name: &str,
        args: (LuaMessage, LuaMessage),
    ) -> <Self as Handler<LuaMessage>>::Result {
        self.vm.scope(|scope| {
            let globals = self.vm.globals();

            let rpc = scope
                .create_function_mut(|_, msg| {
                    ctx.notify(msg);
                    Ok(())
                })
                .unwrap();
            globals.set("__rpc", rpc).unwrap();

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

fn invoke(
    ctx: &mut Context<LuaActor>,
    vm: &mut Lua,
    recs: &mut HashMap<String, Recipient<LuaMessage>>,
    func_name: &str,
    args: Vec<LuaMessage>,
) -> LuaMessage {
    let ctx = RefCell::new(ctx);
    let iter = args.into_iter()
        .map(|msg| msg.to_lua(&vm).unwrap())
        .collect();
    let args = MultiValue::from_vec(iter);
    // We can't create a function with references to `self` and is 'static since `self` already owns Lua.
    // A function within Lua owning `self` creates self-borrowing cycle.
    // Also, Lua requires all values passed to it is 'static because we can't know when will Lua GC our value.
    // Therefore, we use scope to make sure the `__rpc` function is temporary and don't have to deal with 'static lifetime.
    //
    // (Quote from: https://github.com/kyren/rlua/issues/56#issuecomment-363928738
    // When the scope ends, the Lua function is 100% guaranteed (afaict!) to be "invalidated".
    // This means that calling the function will cause an immediate Lua error with a message like "error, call of invalidated function".)
    //
    // for reference, check https://github.com/kyren/rlua/issues/73#issuecomment-370222198
    vm.scope(|scope| {
        let globals = vm.globals();

        let notify = scope
            .create_function_mut(|_, msg| {
                let mut ctx = ctx.borrow_mut();
                ctx.notify(msg);
                Ok(())
            })
            .unwrap();
        globals.set("notify", notify).unwrap();
        let notify_later = scope
            .create_function_mut(|_, (msg, secs)| {
                let mut ctx = ctx.borrow_mut();
                ctx.notify_later(msg, Duration::new(secs, 0));
                Ok(())
            })
            .unwrap();
        globals.set("notify_later", notify_later).unwrap();
        let new_actor = scope
            .create_function_mut(|_, (script_path, cb_thread_id): (String, u64)| {
                let recipient_id = Uuid::new_v4();
                let name = format!("LuaActor-{}-{}", recipient_id, &script_path);

                let addr = LuaActor::new_from_file(&script_path).unwrap().start();
                // TODO: fix this line
                recs.insert(name.clone(), addr.recipient());
                Ok(name.clone())
            })
            .unwrap();
        globals.set("new_actor", new_actor).unwrap();

        let lua_handle: Result<Function, LuaError> = globals.get(func_name);
        if let Ok(f) = lua_handle {
            LuaMessage::from_lua(f.call::<MultiValue, Value>(args).unwrap(), &vm).unwrap()
        } else {
            LuaMessage::Nil
        }
    })
}

impl Actor for LuaActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        // self.invoke_in_scope(ctx, "started", LuaMessage::Nil);
    }

    fn stopped(&mut self, ctx: &mut Context<Self>) {
        // self.invoke_in_scope(ctx, "stopped", LuaMessage::Nil);
    }
}

impl Handler<LuaMessage> for LuaActor {
    type Result = LuaMessage;

    fn handle(&mut self, msg: LuaMessage, ctx: &mut Context<Self>) -> Self::Result {
        let handle_script = self.handle_script.clone();
        invoke(
            ctx,
            &mut self.vm,
            &mut self.recipients,
            "__run",
            vec![handle_script, msg],
        )
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
    fn lua_actor_basic() {
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
    fn lua_actor_return_table() {
        let system = System::new("test");

        let lua_addr = LuaActor::new(
            r#"
        return {x = 1}
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
        if not ctx.state then ctx.state = 0 end

        ctx.state = ctx.state + 1
        return ctx.state
        "#,
        ).unwrap()
            .start();

        let l = lua_addr.send(LuaMessage::Nil);
        Arbiter::spawn(l.map(move |res| {
            assert_eq!(res, LuaMessage::from(1));
            let l2 = lua_addr.send(LuaMessage::Nil);
            Arbiter::spawn(l2.map(|res| {
                assert_eq!(res, LuaMessage::from(2));
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
