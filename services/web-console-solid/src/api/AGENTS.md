# api/ — how the console talks to the server

## Purpose

One file per backend resource. Each file knows the URL, the method, and the
request and response shape. Nothing here knows about a screen, a component, or a
signal.

This layer exists so a change to an endpoint is one edit in one place, and so a
component never contains a URL.

## Rules

1. No `fetch` outside `client.js`. The client adds the CSRF header, the
   environment header, and the error mapping once.
2. A function returns parsed data or throws `ApiError`. It never returns a
   response object, because that would leak HTTP details into a component.
3. No state. These are functions, not hooks; state lives in `features/*/use*.js`
   or `state/`.
4. A path is written once. If two features need the same call, they import the
   same function.

## Files

| File | Resource |
|---|---|
| `client.js` | fetch wrapper, CSRF, environment header, error mapping |
| `auth.js` | login, logout, identity, environments |
| `sandboxes.js` | list, detail, create, actions, logs, exec, inference, rules |
| `policies.js` | policies, rules, drafts, traffic windows |
| `telemetry.js` | history, facets, the SSE stream |
| `cluster.js` | gateways, settings, sync, supervisors, topology |
| `providers.js` | profiles, probe; locators only, never a value |
| `models.js` | catalog with filters, hardware, facets, requirements, install, uninstall |
| `jobs.js` | jobs, logs, schedules |

## Extending

Add the function to the file of its resource. If the resource is new, add a file
and a row to the table above.
