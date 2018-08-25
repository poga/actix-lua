local router = require("router")
local r = router.new()
local liluat = require("liluat")

local file = io.open("./templates/index.tmpl", "r")
local content = file:read("*a")
local tmpl = liluat.compile(content)

r:match('GET', '/hello', function (params)
  return liluat.render(tmpl, {title="hello world", verb="Hello "})
end)

r:match('POST', '/form', function (params)
  return "submitted"
end)

local ok, ret = r:execute(ctx.msg.method, '/' .. ctx.msg.path)

if not ok then
  -- TODO: set status code
  ret = "404 not found"
end

return ret

-- return "hi! " .. ctx.msg