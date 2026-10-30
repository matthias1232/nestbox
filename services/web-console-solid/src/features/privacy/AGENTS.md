# privacy — the content-privacy (DLP) layer

## Purpose

One screen for the question "what is inspected on its way into and out of a
sandbox, and what happens to a finding?". The top half is the policy for the
environment default or one sandbox; the bottom half tests that policy once and
shows the incidents it recorded.

## Files

| File | Responsibility |
|---|---|
| `PrivacyPage.jsx` | Scope picker, mode, detectors with their actions, egress and retention, the inspection test, incidents, the admin purge |
| `usePrivacy.js` | Loading, the draft, saving, inspecting, purging, error text; no JSX |

## Rules

1. A blank sandbox means "environment default", never "every sandbox". The picker
   has an explicit option for it, and inspection refuses to run without a
   sandbox, because the sandbox's policy is what answers.
2. The mode is a decision, not a label: each mode is offered with one sentence
   that says what it does, and no sentence claims more than the server does.
3. `allow` is not hidden. The page says plainly that the server refuses it for a
   secret detector, so a refused save is expected rather than surprising.
4. The inspection box tests the filter; it is not a conversation. The page says
   so, and the result shows the transformed text and the masked findings instead
   of implying that something was sent.
5. Incidents carry masked values only. The page states that the original value
   is never stored, and never renders a field that would suggest otherwise.
6. Saving is explicit and admin-only. A changed select is not applied until
   "Save", because the environment default changes what every sandbox inherits.
7. The purge asks once and says what it deletes: incident rows older than the
   day count, for the selected sandbox.

## Wiring

`App.jsx` is not part of this package: the integrator adds the route and the
`features/AGENTS.md` row. The page takes a `role` prop and hides the save and
purge controls for anyone who is not an admin.
