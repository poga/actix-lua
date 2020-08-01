use ::actix::prelude::*;
use ::actix::ActorContext;
use rlua::Error as LuaError;
use rlua::{FromLua, Function, Lua, MultiValue, ToLua, Value};

use crate::message::LuaMessage;
use std::cell::RefCell;
use std::collections::HashMap;
use std::str;
use std::time::Duration;

/// Top level struct which holds a lua state for itself.
///
/// It provides most of the actix context API to the lua enviroment.
///
/// You can create new `LuaActor` with [`LuaActorBuilder`].
///
///
/// ### `ctx.msg`
/// The message sent to Lua actor.
///
/// ### `ctx.notify(msg)`
/// Send message `msg` to self.
///
/// ### `ctx.notify_later(msg, seconds)`
/// Send message `msg` to self after specified period of time.
///
/// ### `local result = ctx.send(recipient, msg)`
/// Send message `msg` to `recipient asynchronously and wait for response.
///
/// Calling `ctx.send` yield the current coroutine and returns a `ThreadYield(thread_id)` message.
/// LuaActor will wait for the response and resume the yielded coroutine once the response is returned.
///
/// Equivalent to `actix::Recipient.send`.
///
/// ### `ctx.do_send(recipient, msg)`
/// Send message `msg` to `recipient`.
///
/// Equivalent to `actix::Recipient.do_send`.
///
/// ### `ctx.terminate()`
/// Terminate actor execution.
///
/// [`LuaActorBuilder`]: struct.LuaActorBuilder.html
pub struct LuaActor {
    vm: Lua,
    pub recipients: HashMap<String, Recipient<LuaMessage>>,
}

impl LuaActor {
    pub fn new_with_vm(
        vm: Lua,
        started: Option<String>,
        handle: Option<String>,
        stopped: Option<String>,
    ) -> Result<LuaActor, LuaError> {
        let prelude = include_str!("lua/prelude.lua");
        vm.context(|ctx| {
            ctx.load(prelude).set_name("Prelude")?.exec()?;
            {
                let load: Function = ctx.globals().get("__load")?;
                if let Some(script) = started {
                    let res = load.call::<(String, String), ()>((script, "started".to_string()));

                    if let Err(e) = res {
                        return Result::Err(e);
                    }
                }
                if let Some(script) = handle {
                    let res = load.call::<(String, String), ()>((script, "handle".to_string()));

                    if let Err(e) = res {
                        return Result::Err(e);
                    }
                }
                if let Some(script) = stopped {
                    let res = load.call::<(String, String), ()>((script, "stopped".to_string()));

                    if let Err(e) = res {
                        return Result::Err(e);
                    }
                }
            }
            Ok(())
        })?;

        Result::Ok(LuaActor {
            vm,
            recipients: HashMap::new(),
        })
    }

    pub fn new(
        started: Option<String>,
        handle: Option<String>,
        stopped: Option<String>,
    ) -> Result<LuaActor, LuaError> {
        let vm = Lua::new();
        Self::new_with_vm(vm, started, handle, stopped)
    }

    /// Add a recipient to the actor's recipient list.
    /// You can send message to the recipient via `name` with the context API `ctx.send(name, message)`
    pub fn add_recipients(
        &mut self,
        name: &str,
        rec: Recipient<LuaMessage>,
    ) -> Option<Recipient<LuaMessage>> {
        self.recipients.insert(name.to_string(), rec)
    }
}

// Remove all `self` usage with a independent function `invoke`.
fn invoke(
    self_addr: &Recipient<SendAttempt>,
    ctx: &mut Context<LuaActor>,
    vm: &mut Lua,
    recs: &mut HashMap<String, Recipient<LuaMessage>>,
    func_name: &str,
    args: Vec<LuaMessage>,
) -> Result<LuaMessage, LuaError> {
    // `ctx` is used in multiple closure in the lua scope.
    // to create multiple borrow in closures, we use RefCell to move the borrow-checking to runtime.
    // Voliating the check will result in panic. Which shouldn't happend(I think) since lua is single-threaded.
    let ctx = RefCell::new(ctx);
    let recs = RefCell::new(recs);

    vm.context(|lua_ctx| {
        let iter = args
            .into_iter()
            .map(|msg| msg.to_lua(lua_ctx).unwrap())
            .collect();
        let args = MultiValue::from_vec(iter);
        // We can't create a function with references to `self` and is 'static since `self` already owns Lua.
        // A function within Lua owning `self` creates self-borrowing cycle.
        //
        // Also, Lua requires all values passed to it is 'static because we can't know when will Lua GC our value.
        // Therefore, we use scope to make sure these APIs are temporary and don't have to deal with 'static lifetime.
        //
        // (Quote from: https://github.com/kyren/rlua/issues/56#issuecomment-363928738
        // When the scope ends, the Lua function is 100% guaranteed (afaict!) to be "invalidated".
        // This means that calling the function will cause an immediate Lua error with a message like "error, call of invalidated function".)
        //
        // for reference, check https://github.com/kyren/rlua/issues/73#issuecomment-370222198
        lua_ctx.scope(|scope| {
            let globals = lua_ctx.globals();

            let notify = scope.create_function_mut(|_, msg: LuaMessage| {
                let mut ctx = ctx.borrow_mut();
                ctx.notify(msg);
                Ok(())
            })?;
            globals.set("notify", notify)?;

            let notify_later = scope.create_function_mut(|_, (msg, secs): (LuaMessage, u64)| {
                let mut ctx = ctx.borrow_mut();
                ctx.notify_later(msg, Duration::new(secs, 0));
                Ok(())
            })?;
            globals.set("notify_later", notify_later)?;

            let do_send =
                scope.create_function_mut(|_, (recipient_name, msg): (String, LuaMessage)| {
                    let recs = recs.borrow_mut();
                    let rec = recs.get(&recipient_name);

                    // TODO: error handling?
                    if let Some(r) = rec {
                        r.do_send(msg).unwrap();
                    }
                    Ok(())
                })?;
            globals.set("do_send", do_send)?;

            let send = scope.create_function_mut(
                |_, (recipient_name, msg, cb_thread_id): (String, LuaMessage, i64)| {
                    // we can't create a lua function which owns `self`
                    // but `self` is needed for resolving `send` future.
                    //
                    // The workaround is we notify ourself with a `SendAttempt` Message
                    // and resolving `send` future in the `handle` function.
                    self_addr
                        .do_send(SendAttempt {
                            recipient_name,
                            msg,
                            cb_thread_id,
                        })
                        .unwrap();

                    Ok(())
                },
            )?;
            globals.set("send", send)?;

            let terminate = scope.create_function_mut(|_, _: LuaMessage| {
                let mut ctx = ctx.borrow_mut();
                ctx.terminate();
                Ok(())
            })?;
            globals.set("terminate", terminate)?;

            let lua_handle: Result<Function, LuaError> = globals.get(func_name);
            if let Ok(f) = lua_handle {
                match f.call::<MultiValue, Value>(args) {
                    Err(e) => panic!("{:?}", e),
                    Ok(ret) => Ok(LuaMessage::from_lua(ret, lua_ctx).unwrap()),
                }
            } else {
                // return nil if handle is not defined
                Ok(LuaMessage::Nil)
            }
        })
    })
}

impl Actor for LuaActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        if let Err(e) = invoke(
            &ctx.address().recipient(),
            ctx,
            &mut self.vm,
            &mut self.recipients,
            "__run",
            vec![LuaMessage::from("started")],
        ) {
            panic!("lua actor started failed {:?}", e);
        }
    }

    fn stopped(&mut self, ctx: &mut Context<Self>) {
        if let Err(e) = invoke(
            &ctx.address().recipient(),
            ctx,
            &mut self.vm,
            &mut self.recipients,
            "__run",
            vec![LuaMessage::from("stopped")],
        ) {
            panic!("lua actor stopped failed {:?}", e);
        }
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
        if let Ok(res) = invoke(
            &ctx.address().recipient(),
            ctx,
            &mut self.vm,
            &mut self.recipients,
            "__run",
            vec![LuaMessage::from("handle"), msg],
        ) {
            res
        } else {
            LuaMessage::Nil
        }
    }
}

impl Handler<SendAttemptResult> for LuaActor {
    type Result = LuaMessage;

    fn handle(&mut self, result: SendAttemptResult, ctx: &mut Context<Self>) -> Self::Result {
        if let Ok(res) = invoke(
            &ctx.address().recipient(),
            ctx,
            &mut self.vm,
            &mut self.recipients,
            "__resume",
            vec![LuaMessage::from(result.cb_thread_id), result.msg],
        ) {
            res
        } else {
            LuaMessage::Nil
        }
    }
}

impl Handler<SendAttempt> for LuaActor {
    type Result = LuaMessage;

    fn handle(&mut self, attempt: SendAttempt, ctx: &mut Context<Self>) -> Self::Result {
        let rec = &self.recipients[&attempt.recipient_name];
        let self_addr = ctx.address().clone();
        let fut = rec.send(attempt.msg.clone())
            .into_actor(self)
            .then(move |res, _, _| {
                match res {
                    Ok(msg) => self_addr.do_send(SendAttemptResult {
                        msg,
                        cb_thread_id: attempt.cb_thread_id,
                    }),
                    _ => {
                        panic!("send attempt failed: {:?}", res);
                    }
                };
                actix::fut::ok(())
            });
        ctx.wait(fut.map(|_: std::result::Result<(), LuaError>,_,_| ()));
        LuaMessage::Nil
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_timer::Delay;
    use std::collections::HashMap;
    use std::time::Duration;

    use crate::builder::LuaActorBuilder;

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
        let fut = async move {
            let res = l.await;
            match res {
                Ok(res) => {
                    assert_eq!(res, LuaMessage::from(2));
                    System::current().stop();
                }
                Err(e) => {
                    println!("actor dead {}", e);
                }
            };
        };
        Arbiter::spawn(fut);

        system.run();
    }

    #[test]
    fn lua_actor_syntax_error() {
        let res = LuaActorBuilder::new()
            .on_handle_with_lua(r"return 1+")
            .build();

        if let Ok(_) = res {
            panic!("should return Err(syntax_error)");
        }
    }

    #[should_panic]
    #[test]
    fn lua_actor_user_error() {
        let system = System::new("test");

        let lua_addr = lua_actor_with_handle(
            r#"
        print("before")
        error("foo")
        print("after")
        "#,
        )
        .start();

        let l = lua_addr.send(LuaMessage::from(0));
        let fut = async move {
            let res = l.await;
            match res {
                Ok(_res) => {
                    // it should panic. 
                    // and it does, but it seems the test does not pass
                    // running 1 test
                    // thread 'actor::tests::lua_actor_user_error' panicked at ... src/actor.rs:205:31
                    System::current().stop();
                }
                Err(e) => {
                    println!("actor dead {}", e);
                }
            };
        };
        Arbiter::spawn(fut);
        
        system.run();
    }

    #[test]
    fn lua_actor_return_table() {
        let system = System::new("test");

        let lua_addr = lua_actor_with_handle(
            r#"
        return {x = 1}
        "#,
        )
        .start();

        let l = lua_addr.send(LuaMessage::from(3));
        let fut = async move {
            let res = l.await;
            match res {
                Ok(res) => {
                    let mut t = HashMap::new();
                    t.insert("x".to_string(), LuaMessage::from(1));
                    assert_eq!(res, LuaMessage::from(t));
                    System::current().stop();
                }
                Err(e) => {
                    println!("actor dead {}", e);
                }
            };
        };
        Arbiter::spawn(fut);

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
        )
        .start();

        let l = lua_addr.send(LuaMessage::Nil);
        let fut = async move {
            let res = l.await;
            match res {
                Ok(res) => {
                    assert_eq!(res, LuaMessage::from(1));
                    let l2 = lua_addr.send(LuaMessage::Nil);
                    let fut = async move {
                        let res = l2.await;
                        match res {
                            Ok(res) => {
                                assert_eq!(res, LuaMessage::from(2));
                                System::current().stop();
                            }
                            Err(e) => {
                                println!("actor dead {}", e);
                            }
                        };
                    };
                    Arbiter::spawn(fut);
                }
                Err(e) => {
                    println!("actor dead {}", e);
                }
            };
        };
        Arbiter::spawn(fut);

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

        let fut = async move {
            let _ = Delay::new(Duration::from_secs(1)).await.map(move |()| {
                let l = addr.send(LuaMessage::from(1));
                let fut = async move {
                    let res = l.await;
                    match res {
                        Ok(res) => {
                            assert_eq!(res, LuaMessage::from(101));
                            System::current().stop();
                        }
                        Err(e) => {
                            println!("actor dead {}", e);
                        }
                    };
                };
                Arbiter::spawn(fut)
            });
        };
        Arbiter::spawn(fut);

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
        let fut = async move {
            let _ = Delay::new(Duration::from_secs(2)).await.map(move |()| {
                let l2 = addr.send(LuaMessage::from(1));
                let fut = async move {
                    let res = l2.await;
                    match res {
                        Ok(res) => {
                            assert_eq!(res, LuaMessage::from(101));
                            System::current().stop();
                        }
                        Err(e) => {
                            println!("actor dead {}", e);
                        }
                    };
                };
                Arbiter::spawn(fut)
            });
        };
        
        Arbiter::spawn(fut);
        system.run();
    }

    #[test]
    fn lua_actor_send() {
        use std::mem::discriminant;
        let system = System::new("test");

        struct Callback;
        impl Actor for Callback {
            type Context = Context<Self>;
        }

        impl Handler<LuaMessage> for Callback {
            type Result = LuaMessage;

            fn handle(&mut self, msg: LuaMessage, _ctx: &mut Context<Self>) -> Self::Result {
                // check msg type
                assert_eq!(
                    discriminant(&msg),
                    discriminant(&LuaMessage::String("foo".to_string()))
                );
                if let LuaMessage::String(s) = msg {
                    assert_eq!(s, "Hello");
                    System::current().stop();
                    LuaMessage::Boolean(true)
                } else {
                    unimplemented!()
                }
            }
        }
        let callback_addr = Callback.start();

        let mut actor = LuaActorBuilder::new()
            .on_started_with_lua(
                r#"
            local result = ctx.send("callback", "Hello")
            print("result", "=", result)
            "#,
            )
            .build()
            .unwrap();

        actor.add_recipients("callback", callback_addr.recipient());
        actor.start();
        system.run();
    }

    #[test]
    fn lua_actor_thread_yield() {
        use std::mem::discriminant;
        struct Callback;
        impl Actor for Callback {
            type Context = Context<Self>;
        }

        impl Handler<LuaMessage> for Callback {
            type Result = LuaMessage;

            fn handle(&mut self, _: LuaMessage, _ctx: &mut Context<Self>) -> Self::Result {
                LuaMessage::Nil
            }
        }

        let system = System::new("test");

        let mut actor = LuaActorBuilder::new()
            .on_handle_with_lua(
                r#"
            local result = ctx.send("callback", "Hello")
            print(result)
            return result
            "#,
            )
            .build()
            .unwrap();

        actor.add_recipients("callback", Callback.start().recipient());

        let addr = actor.start();

        let l = addr.send(LuaMessage::Nil);
        let fut = async move {
            let res = l.await;
            match res {
                Ok(res) => {
                    assert_eq!(
                        discriminant(&res),
                        discriminant(&LuaMessage::ThreadYield("foo".to_string()))
                    );
                    System::current().stop();
                }
                Err(e) => {
                    println!("actor dead {}", e);
                }
            };
        };
        Arbiter::spawn(fut);
        system.run();
    }

    #[test]
    fn lua_actor_thread_yield_and_callback_message() {
        use std::mem::discriminant;

        struct Callback;
        impl Actor for Callback {
            type Context = Context<Self>;
        }

        impl Handler<LuaMessage> for Callback {
            type Result = LuaMessage;

            fn handle(&mut self, msg: LuaMessage, _ctx: &mut Context<Self>) -> Self::Result {
                // check msg type
                assert_eq!(
                    discriminant(&msg),
                    discriminant(&LuaMessage::String("foo".to_string()))
                );
                if let LuaMessage::String(s) = msg {
                    assert_eq!(s, "Hello");
                    LuaMessage::String(format!("{} from callback", s))
                } else {
                    unimplemented!()
                }
            }
        }

        struct Check;
        impl Actor for Check {
            type Context = Context<Self>;
        }

        impl Handler<LuaMessage> for Check {
            type Result = LuaMessage;

            fn handle(&mut self, msg: LuaMessage, _ctx: &mut Context<Self>) -> Self::Result {
                // check msg type
                assert_eq!(
                    discriminant(&msg),
                    discriminant(&LuaMessage::String("foo".to_string()))
                );
                if let LuaMessage::String(s) = msg {
                    assert_eq!(s, "Hello from callback");
                    System::current().stop();
                    LuaMessage::Nil
                } else {
                    unimplemented!()
                }
            }
        }

        let system = System::new("test");
        let mut actor = LuaActorBuilder::new()
            .on_handle_with_lua(
                r#"
            local result = ctx.send("callback", ctx.msg)
            print("send result", "=", result)
            ctx.send("check", result)
            "#,
            )
            .build()
            .unwrap();

        actor.add_recipients("callback", Callback.start().recipient());
        actor.add_recipients("check", Check.start().recipient());

        let addr = actor.start();

        let l = addr.send(LuaMessage::String("Hello".to_string()));
        let fut = async move {
            let res = l.await;
            match res {
                Ok(res) => {
                    assert_eq!(
                        discriminant(&res),
                        discriminant(&LuaMessage::ThreadYield("foo".to_string()))
                    );
                    System::current().stop();
                }
                Err(e) => {
                    println!("actor dead {}", e);
                }
            };
        };
        Arbiter::spawn(fut);

        system.run();
    }

    #[test]
    fn lua_actor_do_send() {
        use std::mem::discriminant;

        struct Check;
        impl Actor for Check {
            type Context = Context<Self>;
        }

        impl Handler<LuaMessage> for Check {
            type Result = LuaMessage;

            fn handle(&mut self, msg: LuaMessage, _ctx: &mut Context<Self>) -> Self::Result {
                // check msg type
                assert_eq!(
                    discriminant(&msg),
                    discriminant(&LuaMessage::String("foo".to_string()))
                );
                if let LuaMessage::String(s) = msg {
                    assert_eq!(s, "Hello");
                    System::current().stop();
                    LuaMessage::Nil
                } else {
                    unimplemented!()
                }
            }
        }
        let system = System::new("test");

        let mut actor = LuaActorBuilder::new()
            .on_handle_with_lua(
                r#"
            local result = ctx.do_send("check", "Hello")
            print("new actor addr name", rec, result)
            return ctx.msg
            "#,
            )
            .build()
            .unwrap();
        actor.add_recipients("check", Check.start().recipient());
        let addr = actor.start();

        let l = addr.send(LuaMessage::Nil);
        let fut = async move {
            let res = l.await;
            match res {
                Ok(res) => {
                    assert_eq!(res, LuaMessage::Nil);
                }
                Err(e) => {
                    println!("actor dead {}", e);
                }
            };
        };
        Arbiter::spawn(fut);
        system.run();
    }

    #[test]
    fn lua_actor_terminate() {
        // TODO: validate on_stopped is called
        let system = System::new("test");

        let _ = LuaActorBuilder::new()
            .on_started_with_lua(
                r#"
            ctx.terminate()
            "#,
            )
            .on_stopped_with_lua(r#"print("stopped")"#)
            .build()
            .unwrap()
            .start();
        
        let fut = async move {
            let res = Delay::new(Duration::from_secs(1)).await.map(move |()| {
                System::current().stop();
            });
            match res {
                Ok(_)=> {}
                Err(e) => {
                    println!("actor dead {}", e);
                }
            };
        };
        Arbiter::spawn(fut);

        system.run();
    }

    use std::env;

    #[test]
    fn lua_actor_require() {
        let system = System::new("test");
        env::set_var("LUA_PATH", "./src/?.lua;;");

        let addr = LuaActorBuilder::new()
            .on_handle_with_lua(
                r#"
                local m = require('lua/test/module')
                return m.incr(ctx.msg)
            "#,
            )
            .build()
            .unwrap()
            .start();
        let l = addr.send(LuaMessage::from(1));
        let fut = async move {
            let res = l.await;
            match res {
                Ok(res) => {
                    assert_eq!(res, LuaMessage::from(2));
                    System::current().stop();
                }
                Err(e) => {
                    println!("actor dead {}", e);
                }
            };
        };
        Arbiter::spawn(fut);

        system.run();
    }

    #[test]
    fn lua_actor_with_vm() {
        let system = System::new("test");

        let vm = Lua::new();
        vm.context(|ctx| {
            ctx.globals()
                .set(
                    "greet",
                    ctx.create_function(|_, name: String| Ok(format!("Hello, {}!", name)))
                        .unwrap(),
                )
                .unwrap();
        });

        let addr = LuaActorBuilder::new()
            .on_handle_with_lua(
                r#"
            return greet(ctx.msg)
            "#,
            )
            .build_with_vm(vm)
            .unwrap()
            .start();

        let l = addr.send(LuaMessage::from("World"));
        let fut = async move {
            let res = l.await;
            match res {
                Ok(res) => {
                    assert_eq!(res, LuaMessage::from("Hello, World!"));
                    System::current().stop();
                }
                Err(e) => {
                    println!("actor dead {}", e);
                }
            };
        };
        Arbiter::spawn(fut);

        system.run();
    }
}
