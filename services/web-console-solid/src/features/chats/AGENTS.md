# chats/ — the model conversations of a sandbox, and of the whole environment

## Purpose

The promise: a conversation reads like a messenger thread, live, with the content
check's work visible on the text itself — what was replaced, what was
pseudonymised, what never left because the policy blocked it.

## Files

| File | Responsibility |
|---|---|
| `ChatBrowser.jsx` | The list (grouped by session, model, day, sandbox or owner) and the selected thread |
| `ChatThread.jsx` | The messenger view: bubbles, tool calls, findings on the text, live growth, copy and export |

## Rules

1. Only masked text is shown. A placeholder is a link to its finding, never a
   reason to guess the value.
2. A blocked turn is visibly blocked: red bubble, struck text, the route it would
   have taken, and a way to allow it on purpose.
3. Copy and export produce the masked text, because that is the only text that
   exists.
