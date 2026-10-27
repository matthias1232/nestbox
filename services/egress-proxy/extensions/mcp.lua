-- Recognize MCP on the wire so a tool call is a fact, not an inference.
--
-- MCP is JSON-RPC 2.0 over HTTP with a known method set. A request that is
-- `Content-Type: application/json`, has `jsonrpc: "2.0"`, and a `method` in the
-- MCP family is MCP; anything else is "some JSON" and is left alone. For
-- `tools/call` the tool name is `params.name`, and `Mcp-Session-Id` names the
-- session.
--
-- The result is written into dynamic metadata (`nestbox:mcp_*`) where the access
-- log and the decider can read it, and the body is handed on unchanged. This
-- filter never rewrites a request: a proxy that edits what an agent sends is a
-- proxy whose logs stop explaining what happened.
--
-- Skeleton note: the scan below is deliberately narrow — it looks for the
-- method and, only for `tools/call`, the first `name` after it. The
-- `feat/egress-envoy` package replaces it with a proper parse and adds the
-- response side (result, is_error, duration). A wrong tool name in a log is
-- worse than none, so anything unrecognised stays empty.

local MCP_METHODS = {
  "initialize",
  "tools/list",
  "tools/call",
  "resources/",
  "prompts/",
  "notifications/",
  "ping",
  "logging/",
  "completion/",
  "sampling/",
  "roots/",
}

local function is_json(headers)
  local content_type = headers:get("content-type")
  if not content_type then
    return false
  end
  return string.find(string.lower(content_type), "application/json", 1, true) ~= nil
end

local function set(handle, name, value)
  if value == nil or value == "" then
    return
  end
  handle:streamInfo():dynamicMetadata():set("nestbox", name, value)
end

local function classify(body, handle)
  local method = string.match(body, '"method"%s*:%s*"([%w%._%-/]+)"')
  if not method then
    return
  end
  local recognised = false
  for _, prefix in ipairs(MCP_METHODS) do
    if string.sub(method, 1, #prefix) == prefix then
      recognised = true
      break
    end
  end
  if not recognised then
    return
  end
  set(handle, "mcp_method", method)
  if method == "tools/call" then
    local after = string.match(body, '"method"%s*:%s*"tools/call"(.*)$')
    local tool = after and string.match(after, '"name"%s*:%s*"([^"]+)"')
    if tool then
      set(handle, "mcp_tool", tool)
    end
  end
end

function envoy_on_request(handle)
  local headers = handle:headers()
  set(handle, "mcp_session_id", headers:get("mcp-session-id"))
  if not is_json(headers) then
    return
  end
  -- Envoy raises when the body was not buffered (over the limit, or a stream).
  -- That is not an error worth failing the request for: it is a body this
  -- filter cannot see, and the log will show empty MCP fields.
  local ok, body = pcall(function() return handle:body() end)
  if not ok or body == nil then
    return
  end
  -- Envoy hands over the buffered body when the buffer limit was not exceeded.
  -- A body larger than the limit is not classified, and the log says so by
  -- leaving the MCP fields empty.
  classify(body:getBytes(0, body:length()), handle)
end
