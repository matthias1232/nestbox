// A bounded list of live events, keyed so a later event updates its row.
export function createTicker({ max = 300 } = {}) {
  let items = [];
  return {
    items: () => items,
    upsert(event, key) {
      const id = key(event);
      const existing = items.find((item) => key(item) === id);
      const merged = existing ? { ...existing, ...event } : event;
      items = [merged, ...items.filter((item) => key(item) !== id)].slice(0, max);
      return merged;
    },
    remove(keyOf) {
      items = items.filter((item) => keyOf(item) !== keyOf(items[0] || {}));
    },
    clear() {
      items = [];
    },
  };
}
