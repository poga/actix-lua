local router = require("router")
local r = router.new()
local liluat = require("liluat")

local file = io.open("./templates/index.tmpl", "r")
local content = file:read("*a")
local tmpl = liluat.compile(content)

local ret

r:match('GET', '/hello', function (params)
  local html = liluat.render(tmpl, {title="hello world", verb="Hello "})

  ret = html
end)

r:match('POST', '/form', function (params)
  ret = "submitted"
end)

if not r:execute(ctx.msg.method, '/' .. ctx.msg.path) then
  -- TODO: set status code
  ret = "404 not found"
end

return ret

-- return "hi! " .. ctx.msg