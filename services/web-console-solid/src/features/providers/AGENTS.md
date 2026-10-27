# providers/ — model endpoints

## Purpose

Show and edit the places a sandbox can send inference requests: the local router,
a cloud API, a vLLM on the overlay. It is the same row shape for all of them, so
"local versus cloud" is a property of the profile, not a separate screen.

## The rule that shapes this screen

**No credential value ever crosses the API.** A profile carries a locator such as
`file:///secrets/omniroute_api_key`; the screen shows `configured` or `missing`
and nothing else. A form field for an API key would turn the control plane into a
secret store, which it is not (ADR 0005).

## What the screen must make obvious

- Which profile a new agent sandbox will use (the first OpenAI-compatible one),
  because that is the endpoint the agent actually talks to.
- Whether the credential resolves: the probe answers "reachable, key accepted"
  or names the specific failure.
- That the endpoint is editable data, so pointing at a different provider is an
  edit, not a redeploy.

## Files

| File | Responsibility |
|---|---|
| `ProvidersPage.jsx` | List, create, edit, probe |
