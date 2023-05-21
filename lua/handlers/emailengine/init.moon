inspect = require "lib.inspect"
json = require "lib.json"
url = require "lib.url"
str = require "lib.str"

class EmailEngine
    name: "emailengine_app"
    description: "Handles emailengine.app emails"

    new: (config = {}) =>
        @secret = config.emailengine_secret or nil
        if config.emailengine_accounts != nil and config.emailengine_accounts != ""
            @ee_accounts = str.explode(",", config.emailengine_accounts)
        else
            @ee_accounts = {}

    has_recipient: (body, address, direction = "to") =>
        addresses = body.data[direction]
        return false if not addresses

        for recipient in *addresses
            if type(address) == "function"
                return true if address recipient.address
            elseif address == recipient.address
                return true

        false

    authorize: (request) =>
        if request.method != "POST"
            return false

        u = url.parse request.url
        str.endswith u.path, "/handle/emailengine"

        if @secret != nil
            authorization = request.headers["authorization"]
            if authorization == nil
                return false

            _, token = str.explode(" ", authorization)
            if token != @secret
                return false

    route: (request) =>
        body = json.decode request.body
        -- print inspect.inspect body

        assert body.account, "No account information in payload"
        return nil if not @ee_accounts[body.account]

        -- Special case for invoices addresses
        is_invoice = (address) ->
            return true if address == "invoices@zentria.ee"
            return true if endswith address, "@zentria.ee"
            return true if endswith address, "@zentria.company"
            false

        return "new_invoices" if @has_recipient body, is_invoice

        "new_emails"

    queue_names: () =>
        {"new_invoices", "new_emails"}
