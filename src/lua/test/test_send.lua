-- print("new actor recived", ctx.msg)
if ctx.msg == "Hello" then
  ctx.state.ok = true
end

return ctx.state.ok