// Counter arithmetic for the live page.
//
// Kept as free functions so the hook, the header strip, and the side panel
// cannot disagree about what "requests in five minutes" means.

// The window the snapshot's `*_5m` fields describe, and the window a token
// count is rolled over locally.
export const WINDOW_MS = 5 * 60 * 1000;

function finite(value) {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0;
}

// A snapshot value plus the events received after that snapshot arrived.
export function requests5m(agent) {
  return finite(agent.baseRequests) + finite(agent.liveRequests);
}

// A cost the server never reported is `null`, not `$0`: an unreported value and
// a free request are different facts.
export function costMicros5m(agent) {
  if (!agent.costReported) return null;
  return finite(agent.baseCost) + finite(agent.liveCost);
}

// Tokens are not in the snapshot, so they are counted from the stream over a
// local five-minute window. No reported token at all is `null`, not zero.
export function tokens5m(agent, now = Date.now()) {
  const window = (agent.tokenWindow || []).filter((entry) => now - entry.atMs <= WINDOW_MS);
  if (window.length === 0) return null;
  return window.reduce((sum, entry) => sum + finite(entry.tokens), 0);
}

export function organisationTotals(agents, now = Date.now()) {
  const totals = { agents: agents.length, active: 0, requests: 0, cost: null, tokens: null };
  let costSeen = false;
  let cost = 0;
  let tokensSeen = false;
  let tokens = 0;
  for (const agent of agents) {
    totals.requests += requests5m(agent);
    if (agent.current) totals.active += 1;

    const agentCost = costMicros5m(agent);
    if (agentCost !== null) {
      cost += agentCost;
      costSeen = true;
    }

    const agentTokens = tokens5m(agent, now);
    if (agentTokens !== null) {
      tokens += agentTokens;
      tokensSeen = true;
    }
  }
  if (costSeen) totals.cost = cost;
  if (tokensSeen) totals.tokens = tokens;
  return totals;
}
