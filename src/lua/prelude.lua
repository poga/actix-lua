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
