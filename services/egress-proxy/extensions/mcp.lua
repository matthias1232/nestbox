-- Recognize MCP on the wire so a tool call is a fact, not an inference.
--
-- MCP is JSON-RPC 2.0 over HTTP with a known method set. A request that is
-- `Content-Type: application/json`, has `jsonrpc: "2.0"`, and a `method` in the
-- MCP family is MCP; anything else is "some JSON" and is left alone. For
-- `tools/call` the tool name is `params.name`, and `Mcp-Session-Id` names the
-- session (the request header, or the one the server assigns in its response).
--
-- The result is written into dynamic metadata (`nestbox:*`) where the access
-- log and the decider can read it, and the body is handed on unchanged. This
-- filter never rewrites a request: a proxy that edits what an agent sends is a
-- proxy whose logs stop explaining what happened.
--
-- The Lua filter has no JSON library, so both sides are matched with patterns
-- over the buffered body. That has two consequences worth stating plainly:
--
--   * A batch (`[{...},{...}]`) is classified by its first method only.
--   * The response body is only readable when Envoy buffered it. The buffer
--     filter buffers requests, not responses, so on most responses
--     `handle:body()` raises and the MCP result fields stay empty. The log then
--     still has the method, the tool, the session and the duration — what it
--     does not have is the result, and it does not pretend otherwise.
--
-- A wrong tool name in a log is worse than none, so anything unrecognised
-- stays empty.

-- This node's identity, filled in by start-egress-proxy.sh when the container
-- starts (the token is not in the image). Used for the tunnel check below.
local NODE_ID = "__NESTBOX_NODE_ID__"
local NODE_TOKEN = "__NESTBOX_NODE_TOKEN__"

local METHODS = {
  "initialize",
  "tools/",
  "resources/",
  "prompts/",
  "notifications/",
  "logging/",
  "completion/",
  "sampling/",
  "roots/",
  "elicitation/",
  "ping",
}

local function lower(value)
  return string.lower(value)
end

local function set(handle, name, value)
  if value == nil or value == "" then
    return
  end
  handle:streamInfo():dynamicMetadata():set("nestbox", name, value)
end

local function json(headers)
  local content_type = headers:get("content-type")
  if not content_type then
    return false
  end
  return string.find(lower(content_type), "application/json", 1, true) ~= nil
end

-- Split an authority into host and port, honoring `[2001:db8::1]:443`.
local function split_authority(authority)
  if not authority then
    return nil, nil
  end
  authority = string.gsub(authority, "^%s*", "")
  authority = string.gsub(authority, "%s*$", "")
  if string.sub(authority, 1, 1) == "[" then
    local host, port = string.match(authority, "^%[([^%]]+)%]:?(%d*)$")
    if port == "" then
      port = nil
    end
    return host, port
  end
  local host, port = string.match(authority, "^([^:]+):?(%d*)$")
  if port == "" then
    port = nil
  end
  return host, port
end

local function is_mcp_method(method)
  for _, prefix in ipairs(METHODS) do
    if string.sub(method, 1, #prefix) == prefix then
      return true
    end
  end
  return false
end

-- Return the MCP method and, for `tools/call`, the tool name.
local function classify(body)
  -- `jsonrpc: "2.0"` first: without it this is just a body with a "method".
  if not string.find(body, '"jsonrpc"%s*:%s*"2%.0"') then
    return nil, nil
  end
  local method = string.match(body, '"method"%s*:%s*"([%w%._%-/]+)"')
  if not method or not is_mcp_method(method) then
    return nil, nil
  end
  if method ~= "tools/call" then
    return method, nil
  end
  -- `params.name` is the tool. Search after the `params` key so a `name`
  -- field in a preceding object cannot be mistaken for it.
  local after_method = string.match(body, '"method"%s*:%s*"tools/call"(.*)$')
  if not after_method then
    return method, nil
  end
  local after_params = string.match(after_method, '"params"%s*:%s*{(.*)$') or after_method
  local tool = string.match(after_params, '"name"%s*:%s*"([^"]+)"')
  return method, tool
end

-- Ask the control API whether a CONNECT may open, before a byte is tunnelled.
--
-- Envoy's ext_authz cannot carry a CONNECT to an HTTP authorization service (the
-- request has no path, and the service closes the connection), so the question is
-- asked here with a plain GET. The answer is the same one every other request
-- gets: the control API decides from the destination and the source address.
-- Anything but a clear "200" refuses the tunnel, including no answer at all: a
-- proxy that cannot ask does not guess.
local function check_tunnel(handle, authority, source_address)
  local ok, response_headers = pcall(function()
    local result = handle:httpCall(
      "control_api",
      {
        [":method"] = "GET",
        [":path"] = "/api/v1/internal/policy/check/",
        [":authority"] = authority,
        ["x-nestbox-node"] = NODE_ID,
        ["authorization"] = "Bearer " .. NODE_TOKEN,
        ["x-nestbox-source-ip"] = source_address or "",
        ["user-agent"] = handle:headers():get("user-agent") or "",
      },
      "",
      2000
    )
    return result
  end)
  local meta = handle:streamInfo():dynamicMetadata()
  if not ok or response_headers == nil then
    meta:set("envoy.filters.http.ext_authz", "x-nestbox-decision", "deny")
    meta:set("envoy.filters.http.ext_authz", "x-nestbox-reason", "decider_unreachable")
    handle:respond({ [":status"] = "503", ["x-nestbox-decision"] = "deny" }, "")
    return
  end
  for _, name in ipairs({ "x-nestbox-decision", "x-nestbox-reason", "x-nestbox-rule-id", "x-nestbox-sandbox" }) do
    local value = response_headers[name]
    if value then
      meta:set("envoy.filters.http.ext_authz", name, value)
    end
  end
  -- `transparent` and `hybrid` mean this sandbox may have its TLS read. Mark the
  -- request so the route with the terminating cluster matches; every other value
  -- (including a proxy that does not know the header) stays a blind tunnel.
  -- `replace`, not `add`: a client must not be able to force inspection on itself
  -- by sending the header, because that would only break its own TLS.
  local mode = response_headers["x-nestbox-egress-mode"]
  if mode == "transparent" or mode == "hybrid" then
    handle:headers():replace("x-nestbox-inspect", "1")
  else
    handle:headers():replace("x-nestbox-inspect", "0")
  end
  if response_headers[":status"] ~= "200" then
    handle:respond(
      { [":status"] = "403", ["x-nestbox-decision"] = response_headers["x-nestbox-decision"] or "deny" },
      ""
    )
  end
end

function envoy_on_request(handle)
  -- Who is asking is the connection's address, not anything the client wrote.
  -- Any `x-nestbox-*` header the sandbox sent is removed first, then the source
  -- address is set from the socket, so a sandbox cannot claim to be another.
  local request_headers = handle:headers()
  request_headers:remove("x-nestbox-sandbox")
  request_headers:remove("x-nestbox-sandbox-id")
  request_headers:remove("x-nestbox-node")
  request_headers:remove("x-nestbox-node-token")
  request_headers:remove("x-nestbox-source-ip")
  local peer = handle:streamInfo():downstreamDirectRemoteAddress()
  if peer then
    local address = string.match(peer, "^(.-):%d+$") or peer
    request_headers:add("x-nestbox-source-ip", address)
  end
  local headers = handle:headers()

  local host, port = split_authority(headers:get(":authority"))
  set(handle, "target_host", host)
  set(handle, "target_port", port)

  if headers:get(":method") == "CONNECT" then
    -- A tunnel has no HTTP request inside that this listener can describe, and
    -- no visible ClientHello. What we do know is the name the client asked
    -- for; the log says `connect: true` so nobody reads this as a sniffed SNI.
    set(handle, "connect", "true")
    check_tunnel(handle, headers:get(":authority"), request_headers:get("x-nestbox-source-ip"))
    set(handle, "sni", host)
    -- httpCall yields; header objects fetched before it are invalid after it.
    headers = handle:headers()
  end

  set(handle, "mcp_session_id", headers:get("mcp-session-id"))

  if not json(headers) then
    return
  end

  -- Envoy raises when the body was not buffered (over the limit, or a stream).
  -- That is not an error worth failing the request for: it is a body this
  -- filter cannot see, and the log will show empty MCP fields.
  local ok, body = pcall(function() return handle:body() end)
  if not ok or body == nil then
    return
  end
  local method, tool = classify(body:getBytes(0, body:length()))
  if not method then
    return
  end
  set(handle, "app_protocol", "mcp")
  set(handle, "mcp_method", method)
  set(handle, "mcp_tool", tool)
end

function envoy_on_response(handle)
  local headers = handle:headers()

  -- Servers assign the session on `initialize` and return it as a header. This
  -- is the only MCP fact that shows up on the response side without a body.
  local session = headers:get("mcp-session-id")
  if session then
    set(handle, "mcp_session_id", session)
  end

  local content_type = headers:get("content-type")
  if not content_type or string.find(lower(content_type), "application/json", 1, true) == nil then
    return
  end

  local ok, body = pcall(function() return handle:body() end)
  if not ok or body == nil then
    return
  end
  local text = body:getBytes(0, body:length())
  if text == nil or text == "" then
    return
  end
  if string.find(text, '"isError"%s*:%s*true') then
    set(handle, "mcp_is_error", "true")
    set(handle, "mcp_result_kind", "tool_error")
  elseif string.find(text, '"error"%s*:') then
    set(handle, "mcp_is_error", "true")
    set(handle, "mcp_result_kind", "error")
  elseif string.find(text, '"result"%s*:') then
    set(handle, "mcp_is_error", "false")
    set(handle, "mcp_result_kind", "result")
  end
end
