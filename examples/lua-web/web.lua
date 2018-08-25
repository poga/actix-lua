local router = require("router")
local r = router.new()
local liluat = require("liluat")

local file = io.open("./templates/index.tmpl", "r")
local content = file:read("*a")
local tmpl = liluat.compile(content)

local ret

print(ctx.msg)

r:match('GET', '/hello', function (params)
  print("get!!!")

  local html = liluat.render(tmpl, {title="hello world", verb="Hello "})

  print(html)

  ret = html
end)

r:execute(ctx.msg.method, '/' .. ctx.msg.path)

return ret

-- return "hi! " .. ctx.msg