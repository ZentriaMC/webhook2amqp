known_handlers = {
    "handlers.emailengine",
    "handlers.newreleases",
}

class Router
    new: (config = {}) =>
        @config = config
        @all_handlers = {}

        for handler_path in *known_handlers
            success, error = pcall -> @load_handler @all_handlers, handler_path
            if not success
                print "failed to load handler '#{handler_path}'", error
                continue

            print "loaded handler '#{handler_path}'"

    load_handler: (handlers, path) =>
        handler = require path
        assert handler.name, "Name must be present"
        assert handler.description, "Description must be present"

        instance = handler @config
        assert instance.authorize, "Authorize fn must be present"
        assert instance.route, "Route fn must be present"
        assert instance.queue_names "queue_names fn must be present"

        handlers[handler.name] = instance

    all_queue_names: () =>
        all_names = {}
        for _, handler in pairs @all_handlers
            for name in *handler.queue_names()
                table.insert all_names, name

        all_names

    handle_request: (request) =>
        found_handler = nil

        for name, handler in pairs @all_handlers
            success, authorized = pcall -> handler\authorize(request)
            if not success
                print "handler '#{name}' threw an error during authorize", authorized
                continue

            if not authorized
                continue

            found_handler = handler
            break

        return nil if not found_handler

        success, route_destination = pcall -> found_handler\route(request)
        if not success
            print "handler '#{found_handler.name}' threw an error during route", route_destination
            return nil

        route_destination
