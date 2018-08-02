use actix::prelude::*;
use rlua::Error as LuaError;
use rlua::{FromLua, Function, Lua, MultiValue, ToLua, Value};

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;
use std::str;
use std::time::Duration;
use uuid::Uuid;

use message::LuaMessage;

#[derive(RustEmbed)]
#[folder = "src/lua/"]
struct Asset;

pub struct LuaActorBuilder {
    started: Option<String>,
    handle: Option<String>,
    stopped: Option<String>,
}

impl LuaActorBuilder {
    pub fn new() -> Self {
        let noop = Some("return".to_string());
        LuaActorBuilder {
            started: noop.clone(),
            handle: noop.clone(),
            stopped: noop.clone(),
        }
    }

    pub fn on_started(&mut self, filename: &str) -> &mut Self {
        self.started = Some(read_to_string(filename));
        self
    }

    pub fn on_started_with_lua(&mut self, script: &str) -> &mut Self {
        self.started = Some(script.to_string());
        self
    }

    pub fn on_handle(&mut self, filename: &str) -> &mut Self {
        self.handle = Some(read_to_string(filename));
        self
    }
    pub fn on_handle_with_lua(&mut self, script: &str) -> &mut Self {
        self.handle = Some(script.to_string());
        self
    }

    pub fn on_stopped(&mut self, filename: &str) -> &mut Self {
        self.stopped = Some(read_to_string(filename));
        self
    }

    pub fn on_stopped_with_lua(&mut self, script: &str) -> &mut Self {
        self.stopped = Some(script.to_string());
        self
    }

    pub fn build(&self) -> Result<LuaActor, LuaError> {
        LuaActor::new(
            self.started.clone(),
            self.handle.clone(),
            self.stopped.clone(),
        )
    }
}

fn read_to_string(filename: &str) -> String {
    let mut f = File::open(filename).expect("File not found");
    let mut body = String::new();
    f.read_to_string(&mut body).expect("Failed to read file");

    body
}

pub struct LuaActor {
    vm: Lua,
    pub recipients: HashMap<String, Recipient<LuaMessage>>,
}

impl LuaActor {
    pub fn new(
        started: Option<String>,
        handle: Option<String>,
        stopped: Option<String>,
    ) -> Result<LuaActor, LuaError> {
        let vm = Lua::new();
        // vm.eval::<()>(&script, Some("Init"))?;
        let prelude = Asset::get("prelude.lua").unwrap();
        vm.eval::<()>(&str::from_utf8(&prelude).unwrap(), Some("Prelude"))?;
        {
            let load: Function = vm.globals().get("__load").unwrap();
            if let Some(script) = started {
                load.call::<(String, String), ()>((script, "started".to_string()))
                    .unwrap()
            }
            if let Some(script) = handle {
                load.call::<(String, String), ()>((script, "handle".to_string()))
                    .unwrap()
            }
            if let Some(script) = stopped {
                load.call::<(String, String), ()>((script, "stopped".to_string()))
                    .unwrap()
            }
        }

        Result::Ok(LuaActor {
            vm,
            recipients: HashMap::new(),
        })
    }

    // fn invoke_in_scope(
    //     &mut self,
    //     ctx: &mut Context<Self>,
    //     func_name: &str,
    //     msg: LuaMessage,
    // ) -> <Self as Handler<LuaMessage>>::Result {
    //     invoke(
    //         ctx,
    //         &mut self.vm,
    //         &mut self.recipients,
    //         func_name,
    //         vec![msg],
    //     );
    //     let ctx = RefCell::new(ctx);
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
    //     LuaMessage::Nil
    // }
}

fn invoke(
    self_addr: Recipient<SendAttempt>,
    ctx: &mut Context<LuaActor>,
    vm: &mut Lua,
    recs: &mut HashMap<String, Recipient<LuaMessage>>,
    func_name: &str,
    args: Vec<LuaMessage>,
) -> LuaMessage {
    // `ctx` is used in multiple closure in the lua scope.
    // to create multiple borrow in closures, we use RefCell to move the borrow-checking to runtime.
    // Voliating the check will result in panic. Which shouldn't happend(I think) since lua is single-threaded.
    let ctx = RefCell::new(ctx);
    let recs = RefCell::new(recs);

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
            .create_function_mut(|_, msg: LuaMessage| {
                let mut ctx = ctx.borrow_mut();
                ctx.notify(msg);
                Ok(())
            })
            .unwrap();
        globals.set("notify", notify).unwrap();

        let notify_later = scope
            .create_function_mut(|_, (msg, secs): (LuaMessage, u64)| {
                let mut ctx = ctx.borrow_mut();
                ctx.notify_later(msg, Duration::new(secs, 0));
                Ok(())
            })
            .unwrap();
        globals.set("notify_later", notify_later).unwrap();

        let new_actor = scope
            .create_function_mut(|_, (script_path, name): (String, LuaMessage)| {
                let recipient_id = Uuid::new_v4();
                let mut recipient_name = format!("LuaActor-{}-{}", recipient_id, &script_path);
                if let LuaMessage::String(n) = name {
                    recipient_name = n;
                }

                let addr = LuaActorBuilder::new()
                    .on_handle(&script_path)
                    .build()
                    .unwrap()
                    .start();

                let mut recs = recs.borrow_mut();
                recs.insert(recipient_name.clone(), addr.recipient());
                Ok(recipient_name.clone())
            })
            .unwrap();
        globals.set("__new_actor", new_actor).unwrap();

        let do_send = scope
            .create_function_mut(|_, (recipient_name, msg): (String, LuaMessage)| {
                let recs = recs.borrow_mut();
                let rec = recs.get(&recipient_name);

                // TODO: error handling
                if let Some(r) = rec {
                    r.do_send(msg).unwrap();
                }
                Ok(())
            })
            .unwrap();
        globals.set("do_send", do_send).unwrap();

        let send = scope
            .create_function_mut(
                |_, (recipient_name, msg, cb_thread_id): (String, LuaMessage, i64)| {
                    self_addr
                        .do_send(SendAttempt {
                            recipient_name: recipient_name,
                            msg: msg,
                            cb_thread_id: cb_thread_id,
                        })
                        .unwrap();

                    Ok(())
                },
            )
            .unwrap();
        globals.set("send", send).unwrap();

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
        invoke(
            ctx.address().recipient().clone(),
            ctx,
            &mut self.vm,
            &mut self.recipients,
            "__run",
            vec![LuaMessage::from("started")],
        );
    }

    fn stopped(&mut self, ctx: &mut Context<Self>) {
        // self.invoke_in_scope(ctx, "stopped", LuaMessage::Nil);
    }
}

struct SendAttempt {
    recipient_name: String,
    msg: LuaMessage,
    cb_thread_id: i64,
}

impl Message for SendAttempt {
    type Result = LuaMessage;
}

struct SendAttemptResult {
    msg: LuaMessage,
    cb_thread_id: i64,
}

impl Message for SendAttemptResult {
    type Result = LuaMessage;
}

impl Handler<LuaMessage> for LuaActor {
    type Result = LuaMessage;

    fn handle(&mut self, msg: LuaMessage, ctx: &mut Context<Self>) -> Self::Result {
        invoke(
            ctx.address().recipient().clone(),
            ctx,
            &mut self.vm,
            &mut self.recipients,
            "__run",
            vec![LuaMessage::from("handle"), msg],
        )
    }
}

impl Handler<SendAttemptResult> for LuaActor {
    type Result = LuaMessage;

    fn handle(&mut self, result: SendAttemptResult, ctx: &mut Context<Self>) -> Self::Result {
        println!("send attemplt result {:?}", result.msg);
        invoke(
            ctx.address().recipient().clone(),
            ctx,
            &mut self.vm,
            &mut self.recipients,
            "__resume",
            vec![LuaMessage::from(result.cb_thread_id), result.msg],
        )
    }
}

impl Handler<SendAttempt> for LuaActor {
    type Result = LuaMessage;

    fn handle(&mut self, attempt: SendAttempt, ctx: &mut Context<Self>) -> Self::Result {
        let rec = self.recipients.get(&attempt.recipient_name).unwrap();
        let self_addr = ctx.address().clone();
        rec.send(attempt.msg.clone())
            .into_actor(self)
            .then(move |res, act, _| {
                match res {
                    Ok(msg) => self_addr.do_send(SendAttemptResult {
                        msg: msg,
                        cb_thread_id: attempt.cb_thread_id,
                    }),
                    _ => {
                        println!("send attempt failed");
                    }
                };
                actix::fut::ok(())
            })
            .wait(ctx);

        LuaMessage::Nil
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_timer::Delay;
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::prelude::Future;

    fn lua_actor_with_handle(script: &str) -> LuaActor {
        LuaActorBuilder::new()
            .on_handle_with_lua(script)
            .build()
            .unwrap()
    }

    #[test]
    fn lua_actor_basic() {
        let system = System::new("test");

        let lua_addr = lua_actor_with_handle(r#"return ctx.msg + 1"#).start();

        let l = lua_addr.send(LuaMessage::from(1));
        Arbiter::spawn(l.map(|res| {
            assert_eq!(res, LuaMessage::from(2));
            System::current().stop();
        }).map_err(|e| println!("actor dead {}", e)));

        system.run();
    }

    #[test]
    fn lua_actor_return_table() {
        let system = System::new("test");

        let lua_addr = lua_actor_with_handle(
            r#"
        return {x = 1}
        "#,
        ).start();

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

        let lua_addr = lua_actor_with_handle(
            r#"
        if not ctx.state.x then ctx.state.x = 0 end

        ctx.state.x = ctx.state.x + 1
        return ctx.state.x
        "#,
        ).start();

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

        let addr = LuaActorBuilder::new()
            .on_started_with_lua(
                r#"
            ctx.notify(100)
            "#,
            )
            .on_handle_with_lua(
                r#"
            if ctx.msg == 100 then
                ctx.state.notified = ctx.msg
            end

            return ctx.msg + ctx.state.notified
            "#,
            )
            .build()
            .unwrap()
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

        let addr = LuaActorBuilder::new()
            .on_started_with_lua(
                r#"
            ctx.notify_later(100, 1)
            "#,
            )
            .on_handle_with_lua(
                r#"
            if ctx.msg == 100 then
                ctx.state.notified = ctx.msg
            end

            return ctx.msg + ctx.state.notified
            "#,
            )
            .build()
            .unwrap()
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

        let addr = lua_actor_with_handle(
            r#"
        local id = ctx.new_actor("src/test.lua")
        return id
        "#,
        ).start();
        let l = addr.send(LuaMessage::Nil);
        Arbiter::spawn(l.map(move |res| {
            if let LuaMessage::String(s) = res {
                assert!(s.ends_with("-src/test.lua"));
            } else {
                assert!(false);
            }

            System::current().stop();
        }).map_err(|e| println!("actor dead {}", e)));

        system.run();
    }

    #[test]
    fn lua_actor_send() {
        let system = System::new("test");

        let addr = LuaActorBuilder::new()
            .on_started_with_lua(
                r#"
            local rec = ctx.new_actor("src/test_send.lua", "child")
            ctx.state.rec = rec
            local result = ctx.send(rec, "Hello")
            print("new actor addr name", rec, result)
            "#,
            )
            .on_handle_with_lua(
                r#"
            return ctx.msg
            "#,
            )
            .build()
            .unwrap()
            .start();

        let delay = Delay::new(Duration::from_secs(1)).map(move |()| {
            let l = addr.send(LuaMessage::Nil);
            Arbiter::spawn(l.map(|res| {
                assert_eq!(res, LuaMessage::Nil);
                System::current().stop();
            }).map_err(|e| println!("actor dead {}", e)))
        });
        Arbiter::spawn(delay.map_err(|e| println!("actor dead {}", e)));

        system.run();
    }

    #[test]
    fn lua_actor_do_send() {
        let system = System::new("test");

        let addr = LuaActorBuilder::new()
            .on_started_with_lua(
                r#"
            local rec = ctx.new_actor("src/test_send.lua", "child")
            ctx.state.rec = rec
            local result = ctx.do_send(rec, "Hello")
            print("new actor addr name", rec, result)
            "#,
            )
            .on_handle_with_lua(
                r#"
            return ctx.msg
            "#,
            )
            .build()
            .unwrap()
            .start();

        let delay = Delay::new(Duration::from_secs(1)).map(move |()| {
            let l = addr.send(LuaMessage::Nil);
            Arbiter::spawn(l.map(|res| {
                assert_eq!(res, LuaMessage::Nil);
                System::current().stop();
            }).map_err(|e| println!("actor dead {}", e)))
        });
        Arbiter::spawn(delay.map_err(|e| println!("actor dead {}", e)));

        system.run();
    }
}
