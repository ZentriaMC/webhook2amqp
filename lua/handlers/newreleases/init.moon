inspect = require "lib.inspect"
json = require "lib.json"
sha256 = require "lib.sha256"
str = require "lib.str"
url = require "lib.url"

class NewReleases
    name: "newreleases_io"
    description: "Handles newreleases.io updates"

    new: (config = {}) =>
        @secret_key = config.newreleases_secret_key or nil
        @repos_prerelease = {}

        if config.newreleases_repos_prerelease != nil and config.newreleases_repos_prerelease != ""
            repos = str.explode ",", config.newreleases_repos_prerelease
            for repo in *repos
                @repos_prerelease[repo] = true
        else
            @repos_prerelease = {}

    verify_signature: (signature, timestamp, body) =>
        -- Nothing to do
        return false if not @secret_key

        buf = timestamp .. "." .. body
        computed = sha256.hmac_sha256 @secret_key, buf

        -- TODO: secure compare
        signature == computed

    authorize: (request) =>
        if request.method != "POST"
            return false

        -- u = url.parse request.url
        -- id = str.split "/handle/", u.path

        signature = request.headers["x-newreleases-signature"]
        signature_timestamp = request.headers["x-newreleases-timestamp"]

        if not signature or not signature_timestamp
            return false

        if not @verify_signature signature, signature_timestamp, request.body
            return false

        return true

    route: (request) =>
        body = json.decode request.body
        -- print inspect.inspect body

        assert body.project, "payload does not contain 'project' field"
        project_name = body.project\lower()

        if body.is_prerelease and not @repos_prerelease[project_name]
            return nil

        "newreleases"

    queue_names: () =>
        {"newreleases"}
