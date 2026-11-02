// What the content policy found in this sandbox, and what was changed about it.
//
// The tab answers three questions in the order they are asked: how much was
// found (the dashboard), what exactly happened to each finding (the log, with
// direction and the masked value → substitute), and who changed the policy
// (the history). The policy itself is edited on the Content privacy page; this
// tab shows the effective mode and says plainly whether this sandbox's egress
// mode allows an inline check at all.
import { For, Show, createResource, createSignal } from "solid-js";
import {
  DlpDashboard,
  DlpLog,
  DlpChanges,
  actionWord,
} from "../privacy/DlpPanels.jsx";
import * as privacy from "../../api/privacy.js";

const VIEWS = [
  { id: "findings", label: "Findings" },
  { id: "log", label: "What happened" },
  { id: "changes", label: "Changes" },
];

// Whether a body can be checked on its way out, by the sandbox's egress mode.
function inlineNote(mode) {
  if (mode === "transparent") {
    return "Bodies are checked on their way out for every host this sandbox reaches, and an answer that streams back is recorded but not rewritten.";
  }
  if (mode === "hybrid") {
    return "Bodies are checked for model hosts only; every other host leaves as a tunnel that this layer cannot read.";
  }
  if (mode === "forward_proxy") {
    return "This sandbox reaches the internet through a proxy that keeps TLS intact, so no body is inspected inline. Findings appear only when the agent routes through the accounting proxy.";
  }
  return "This sandbox bypasses the egress proxy, so no body is inspected. Findings appear only when the agent routes through the accounting proxy.";
}

export default function PrivacyTab(props) {
  const [view, setView] = createSignal("findings");
  const [settings] = createResource(
    () => props.sandbox?.id,
    (id) => privacy.settings(id),
  );
  const mode = () => props.sandbox?.egress_mode || "transparent";
  const policy = () => settings()?.settings;

  return (
    <section class="tab-panel">
      <div class="controls">
        <span class="badge">
          <i />
          {policy() ? `policy: ${policy().mode}` : "policy: loading"}
        </span>
        <span class="muted small">
          {policy()
            ? `${policy().detectors.length} detectors · ${policy().actions?.length || 0} overrides · kept ${policy().retention_days} days`
            : ""}
        </span>
        <span class="badge muted">
          <i />
          egress: {mode()}
        </span>
      </div>
      <p class="muted small">{inlineNote(mode())}</p>

      <div class="controls">
        <For each={VIEWS}>
          {(item) => (
            <button
              class={view() === item.id ? "primary tiny" : "secondary tiny"}
              onClick={() => setView(item.id)}
            >
              {item.label}
            </button>
          )}
        </For>
      </div>

      <Show when={view() === "findings"}>
        <DlpDashboard sandboxId={props.sandbox?.id} />
      </Show>
      <Show when={view() === "log"}>
        <DlpLog sandboxId={props.sandbox?.id} limit={200} />
      </Show>
      <Show when={view() === "changes"}>
        <DlpChanges sandboxId={props.sandbox?.id} limit={50} />
      </Show>

      <p class="muted small">
        Every row is one finding by one detector:{" "}
        {["blocked", "redacted", "replaced", "allowed"]
          .map(actionWord)
          .join(", ")}
        . A request is refused when the policy blocks it, rewritten when a
        detector replaces the value, and recorded without a change when the mode
        is observe. Only the masked excerpt is kept.
      </p>
    </section>
  );
}
