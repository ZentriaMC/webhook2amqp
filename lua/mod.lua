local config = assert(CONFIG, "BUG: config is nil")
local R = require("router")(config)

return {
    queue_names = R:all_queue_names(),
    handler = function(request)
        return R:handle_request(request)
    end,
}
