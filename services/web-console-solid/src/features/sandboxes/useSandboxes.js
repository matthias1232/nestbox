// Sandbox list state.
//
// All logic lives here so the page is only markup. That separation is what makes
// paging and filtering testable without a DOM, and it is where the "never ask
// for everything" rule is enforced.

import { createSignal, createMemo } from 'solid-js';
import * as api from '../../api/sandboxes.js';

export function useSandboxes() {
  const [items, setItems] = createSignal([]);
  const [facets, setFacets] = createSignal({});
  const [cursor, setCursor] = createSignal(null);
  const [nextCursor, setNextCursor] = createSignal(null);
  const [total, setTotal] = createSignal(0);
  const [status, setStatus] = createSignal('');
  const [search, setSearch] = createSignal('');
  const [selected, setSelected] = createSignal(new Set());
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal(null);

  async function load(options = {}) {
    setLoading(true);
    setError(null);
    try {
      const page = await api.list({
        limit: 50,
        status: options.status ?? status(),
        search: options.search ?? search(),
        ...(options.cursor === null ? {} : { cursor: options.cursor ?? cursor() }),
      });
      if (options.cursor === null || options.append === false) {
        setItems(page.items || []);
      } else if (options.append) {
        setItems([...items(), ...(page.items || [])]);
      } else {
        setItems(page.items || []);
      }
      setFacets(page.facets || {});
      setNextCursor(page.next_cursor || null);
      setTotal(page.total_estimate || 0);
      setCursor(page.next_cursor || null);
      setSelected(new Set());
    } catch (failure) {
      setError(failure.message);
    } finally {
      setLoading(false);
    }
  }

  function filterByStatus(value) {
    const next = status() === value ? '' : value;
    setStatus(next);
    setCursor(null);
    return load({ status: next, cursor: null });
  }

  function updateSearch(value) {
    setSearch(value);
    setCursor(null);
    return load({ search: value, cursor: null });
  }

  function toggle(id) {
    const next = new Set(selected());
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setSelected(next);
  }

  function toggleAll() {
    if (selected().size === items().length) {
      setSelected(new Set());
    } else {
      setSelected(new Set(items().map((item) => item.id)));
    }
  }

  async function act(id, action) {
    setError(null);
    try {
      await api.act(id, action);
      await load();
    } catch (failure) {
      setError(failure.message);
    }
  }

  async function bulk(action) {
    const ids = [...selected()];
    const results = { ok: 0, failed: 0 };
    for (const id of ids) {
      try {
        await api.act(id, action);
        results.ok += 1;
      } catch {
        // A bulk action reports how many succeeded; one failure does not abort
        // the rest, because stopping at the first error leaves the set half
        // changed with no record of where it stopped.
        results.failed += 1;
      }
    }
    await load();
    if (results.failed > 0) {
      setError(`${results.ok} succeeded, ${results.failed} failed.`);
    }
  }

  async function destroy(id) {
    setError(null);
    try {
      await api.remove(id);
      await load({ cursor: null });
    } catch (failure) {
      setError(failure.message);
    }
  }

  const statusFacet = createMemo(() => facets().status || {});
  const packFacet = createMemo(() => facets().pack || {});

  const canStart = (sandbox) => ['stopped', 'failed'].includes(sandbox.status);
  const canStop = (sandbox) => ['running', 'starting', 'provisioning'].includes(sandbox.status);

  return {
    items, facets, facetsStatus: statusFacet, facetsPack: packFacet,
    total, loading, error, selected, status, search, nextCursor,
    load, filterByStatus, updateSearch, toggle, toggleAll, act, bulk, destroy,
    canStart, canStop, setError,
  };
}
