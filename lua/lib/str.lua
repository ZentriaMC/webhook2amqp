local str = {}

function str.endswith(str, ending)
    return ending == "" or str:sub(-#ending) == ending
end

function str.explode(d, p)
    local t = {}
    local ll = 0
    if (#p == 1) then
       return { p }
    end
    while true do
       l = string.find(p, d, ll, true)
       if l ~= nil then
          table.insert(t, string.sub(p, ll, l - 1))
          ll = l + 1
       else
          table.insert(t, string.sub(p, ll))
          break
       end
    end
    return t
end

return str
