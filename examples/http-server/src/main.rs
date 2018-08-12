extern crate actix;
extern crate actix_lua;
extern crate actix_web;
extern crate env_logger;
extern crate futures;

use actix::prelude::*;
use actix_lua::{LuaActor, LuaActorBuilder, LuaMessage};
use actix_web::{
    http, middleware, server, App, AsyncResponder, FutureResponse, HttpResponse, Path, State,
};
use futures::Future;

struct AppState {
    lua: Addr<LuaActor>,
}

fn index((name, state): (Path<String>, State<AppState>)) -> FutureResponse<HttpResponse> {
    // send async `CreateUser` message to a `DbExecutor`
    state
        .lua
        .send(LuaMessage::from(name.into_inner()))
        .from_err()
        .and_then(|res| match res {
            LuaMessage::String(s) => Ok(HttpResponse::Ok().json(s)),

            // ignore everything else
            _ => unimplemented!(),
        })
        .responder()
}

fn main() {
    ::std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();
    let sys = actix::System::new("actix-lua-example");

    let addr = Arbiter::start(|_| {
        LuaActorBuilder::new()
            .on_handle_with_lua(
                r#"
                    return "hi! " .. ctx.msg
                "#,
            )
            .build()
            .unwrap()
    });

    // Start http server
    server::new(move || {
        App::with_state(AppState{lua: addr.clone()})
            // enable logger
            .middleware(middleware::Logger::default())
            .resource("/{name}", |r| r.method(http::Method::GET).with(index))
    }).bind("127.0.0.1:8080")
        .unwrap()
        .start();

    println!("Started http server: 127.0.0.1:8080");
    let _ = sys.run();
}
