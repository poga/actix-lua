__threads = {}
__scripts = {}

ctx = { state = {} }

function __load(script, name)
    local f, err = load(script, name, "bt")
    if f == nil then
        error(err)
    end
    __scripts[name] = f
end

-- create a new coroutine from given script
function __run(script_name, msg, thread_id)
    ctx.thread_id = thread_id

    ctx.notify = notify
    ctx.notify_later = notify_later
    ctx.new_actor = function (path)
        return __new_actor(path)
    end
    ctx.send = function (recipient_name, msg)
        send(recipient_name, msg, ctx.thread_id)
        return coroutine.yield("__suspended__" .. ctx.thread_id)
    end
    ctx.do_send = do_send
    ctx.terminate = terminate

    ctx.msg = msg

    local thread = coroutine.create(__scripts[script_name])

    local ok, ret = coroutine.resume(thread)
    -- save the thread and its context if the thread yielded
    if coroutine.status(thread) == "suspended" then
        __threads[ctx.thread_id] = { thread = thread, msg = msg }
    end
    ctx.msg = nil
    ctx.thread_id = nil
    return ret
end

-- resume a existing coroutine
function __resume(thread_id, args)
    local thread = __threads[thread_id]
    ctx.thread_id = thread_id
    ctx.msg = thread.msg
    local ok, ret = coroutine.resume(thread.thread, args)
    if coroutine.status(thread.thread) == "dead" then
        __threads[ctx.thread_id] = nil
    end
    ctx.msg = nil
    ctx.thread_id = nil
    return ret
end
