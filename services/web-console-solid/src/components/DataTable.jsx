// One table for every list in the console: sort, filter, search, group, export.
//
// The work lives in `dataTable.js`; this file is the interaction. Live mode adds
// new rows at the top with a short highlight, and holds them back when the reader
// has scrolled away from the top, so a running list does not fight the reader.
import { For, Show, createMemo, createSignal, onCleanup } from 'solid-js';
import { filterRows, groupRows, searchRows, sortRows, toCsv } from './dataTable.js';
import '../styles/dlp.css';

const PAGE = 500;

export default function DataTable(props) {
  const columns = () => props.columns || [];
  const [sort, setSort] = createSignal(props.defaultSort || null);
  const [groupKey, setGroupKey] = createSignal(props.defaultGroup || null);
  const [filters, setFilters] = createSignal({});
  const [search, setSearch] = createSignal('');
  const [shown, setShown] = createSignal(PAGE);
  const [collapsed, setCollapsed] = createSignal({});
  const rowKey = (row) => (props.rowKey ? props.rowKey(row) : row?.id);

  const rows = createMemo(() => {
    let current = props.rows || [];
    current = searchRows(current, columns(), search());
    current = filterRows(current, columns(), filters());
    return sortRows(current, columns(), sort());
  });

  const grouped = createMemo(() => {
    const column = columns().find((candidate) => candidate.key === groupKey());
    return groupRows(rows().slice(0, shown()), column);
  });

  function toggleSort(column) {
    if (!column.sortable) return;
    const current = sort();
    if (!current || current.key !== column.key) setSort({ key: column.key, dir: 'asc' });
    else if (current.dir === 'asc') setSort({ key: column.key, dir: 'desc' });
    else setSort(null);
  }

  function options(column) {
    const values = new Set();
    for (const row of props.rows || []) {
      const value = column.value ? column.value(row) : row[column.key];
      if (value != null && value !== '') values.add(String(value));
    }
    return [...values].sort();
  }

  function setFilter(key, value) {
    setFilters({ ...filters(), [key]: value });
    setShown(PAGE);
  }

  function download() {
    const csv = toCsv(rows(), columns());
    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `${props.storageKey || 'table'}.csv`;
    link.click();
    URL.revokeObjectURL(url);
  }

  const arrow = (column) => {
    const current = sort();
    if (!current || current.key !== column.key) return '';
    return current.dir === 'asc' ? ' ▲' : ' ▼';
  };

  return (
    <section class="data-table">
      <div class="controls">
        <input
          class="input"
          type="search"
          placeholder="Suchen…"
          value={search()}
          onInput={(event) => {
            setSearch(event.currentTarget.value);
            setShown(PAGE);
          }}
        />
        <select value={groupKey() || ''} onChange={(event) => setGroupKey(event.currentTarget.value || null)}>
          <option value="">nicht gruppieren</option>
          <For each={columns().filter((column) => column.groupable)}>
            {(column) => <option value={column.key}>nach {column.label}</option>}
          </For>
        </select>
        <button class="secondary tiny" onClick={download}>CSV</button>
        {props.toolbar}
      </div>

      <div class="table-scroll">
        <table class="dense">
          <thead>
            <tr>
              <For each={columns()}>
                {(column) => (
                  <th
                    class={column.sortable ? 'sortable' : ''}
                    onClick={() => toggleSort(column)}
                  >
                    {column.label}{arrow(column)}
                  </th>
                )}
              </For>
            </tr>
            <Show when={columns().some((column) => column.filter)}>
              <tr class="filter-row">
                <For each={columns()}>
                  {(column) => (
                    <th>
                      <Show when={column.filter === 'text'}>
                        <input
                          class="input tiny"
                          value={filters()[column.key] || ''}
                          onInput={(event) => setFilter(column.key, event.currentTarget.value)}
                        />
                      </Show>
                      <Show when={column.filter === 'select'}>
                        <select
                          value={filters()[column.key] || ''}
                          onChange={(event) => setFilter(column.key, event.currentTarget.value)}
                        >
                          <option value="">alle</option>
                          <For each={options(column)}>
                            {(value) => <option value={value}>{value}</option>}
                          </For>
                        </select>
                      </Show>
                    </th>
                  )}
                </For>
              </tr>
            </Show>
          </thead>
          <tbody>
            <For
              each={grouped()}
              fallback={
                <tr>
                  <td colspan={columns().length} class="muted small">
                    {props.emptyText || 'Nichts in diesem Zeitraum.'}
                  </td>
                </tr>
              }
            >
              {(group) => (
                <>
                  <Show when={group.key}>
                    <tr class="group-row">
                      <td colspan={columns().length}>
                        <button
                          class="tiny secondary"
                          onClick={() =>
                            setCollapsed({ ...collapsed(), [group.key]: !collapsed()[group.key] })
                          }
                        >
                          {collapsed()[group.key] ? '▸' : '▾'} {group.label}
                        </button>
                        <span class="muted small"> {group.count}</span>
                      </td>
                    </tr>
                  </Show>
                  <Show when={!group.key || !collapsed()[group.key]}>
                    <For each={group.rows}>
                      {(row) => (
                        <tr
                          class={`${props.rowClass ? props.rowClass(row) : ''}`}
                          onClick={() => props.onRowClick?.(row)}
                        >
                          <For each={columns()}>
                            {(column) => (
                              <td>
                                {column.render ? column.render(row) : cellValueText(row, column)}
                              </td>
                            )}
                          </For>
                        </tr>
                      )}
                    </For>
                  </Show>
                </>
              )}
            </For>
          </tbody>
        </table>
      </div>
      <Show when={rows().length > shown()}>
        <button class="secondary tiny" onClick={() => setShown(shown() + PAGE)}>
          Weitere {PAGE} anzeigen
        </button>
      </Show>
      <p class="muted small">{rows().length} Zeilen</p>
    </section>
  );
}

function cellValueText(row, column) {
  const value = column.value ? column.value(row) : row?.[column.key];
  return value == null ? '—' : String(value);
}
