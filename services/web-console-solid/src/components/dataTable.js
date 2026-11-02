// Sorting, filtering, searching, grouping and CSV for the shared table.
//
// Pure functions, so the table's behaviour is tested without a browser. A column
// says how to read a row (`value`), how to draw it (`render`), whether it can be
// sorted, grouped or filtered, and with what kind of filter.

export function cellValue(row, column) {
  if (typeof column.value === 'function') return column.value(row);
  return row?.[column.key];
}

function comparable(value) {
  if (value == null) return null;
  if (typeof value === 'number') return value;
  if (value instanceof Date) return value.getTime();
  const text = String(value);
  const asDate = /^\d{4}-\d{2}-\d{2}T/.test(text) ? Date.parse(text) : Number.NaN;
  if (!Number.isNaN(asDate)) return asDate;
  const asNumber = Number(text);
  if (text.trim() !== '' && !Number.isNaN(asNumber)) return asNumber;
  return text.toLowerCase();
}

export function sortRows(rows, columns, sort) {
  if (!sort?.key) return rows;
  const column = columns.find((candidate) => candidate.key === sort.key);
  if (!column) return rows;
  const direction = sort.dir === 'desc' ? -1 : 1;
  return [...rows].sort((left, right) => {
    const a = comparable(cellValue(left, column));
    const b = comparable(cellValue(right, column));
    if (a == null && b == null) return 0;
    if (a == null) return 1;
    if (b == null) return -1;
    if (typeof a === 'string' && typeof b === 'string') {
      return direction * a.localeCompare(b, undefined, { numeric: true, sensitivity: 'base' });
    }
    if (a === b) return 0;
    return direction * (a < b ? -1 : 1);
  });
}

export function filterRows(rows, columns, filters = {}) {
  const active = Object.entries(filters).filter(([, value]) =>
    Array.isArray(value) ? value.length > 0 : value != null && value !== '',
  );
  if (!active.length) return rows;
  return rows.filter((row) =>
    active.every(([key, wanted]) => {
      const column = columns.find((candidate) => candidate.key === key);
      if (!column) return true;
      const value = cellValue(row, column);
      const text = value == null ? '' : String(value).toLowerCase();
      if (Array.isArray(wanted)) return wanted.some((entry) => entry.toLowerCase() === text);
      return text.includes(String(wanted).toLowerCase());
    }),
  );
}

export function searchRows(rows, columns, q) {
  const needle = String(q || '').trim().toLowerCase();
  if (!needle) return rows;
  return rows.filter((row) =>
    columns.some((column) => {
      const value = cellValue(row, column);
      return value != null && String(value).toLowerCase().includes(needle);
    }),
  );
}

export function groupRows(rows, column) {
  if (!column) return [{ key: '', label: '', rows, count: rows.length }];
  const groups = new Map();
  for (const row of rows) {
    const value = cellValue(row, column);
    const key = value == null || value === '' ? '(none)' : String(value);
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(row);
  }
  return [...groups.entries()]
    .map(([key, members]) => ({ key, label: key, rows: members, count: members.length }))
    .sort((left, right) => right.count - left.count || left.key.localeCompare(right.key));
}

export function toCsv(rows, columns) {
  const escape = (value) => {
    if (value == null) return '';
    const text = String(value);
    if (/[",\r\n]/.test(text)) return `"${text.replaceAll('"', '""')}"`;
    return text;
  };
  const head = columns.map((column) => escape(column.label ?? column.key)).join(',');
  const body = rows.map((row) =>
    columns.map((column) => escape(cellValue(row, column))).join(','),
  );
  return [head, ...body].join('\r\n');
}
