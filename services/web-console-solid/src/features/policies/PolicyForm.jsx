// A policy document as a form, rendered from `/policy-schema`.
//
// The server says which sections exist and what each field is, so a new field
// shows up here without a console release. The document itself stays plain JSON:
// `props.spec` in, `props.onChange(next)` out, no hidden state. A list section
// (filesystem paths, Landlock rules) is a table of rows, the process section is a
// set of fields, and the network section is named groups of endpoints.

import { For, Show } from 'solid-js';
import { blankEndpoint, coerce, freshGroupName, joinList, renameGroup, withField } from './policyForms.js';
import '../../styles/policy-forms.css';

// One schema field as an input, bound to `value` and reporting a coerced value.
function FieldInput(props) {
  const field = () => props.field;
  const value = () => props.value;
  const report = (raw) => props.onChange(coerce(field(), raw));

  return (
    <Show
      when={field().type === 'enum'}
      fallback={
        <Show
          when={field().type === 'enum-list'}
          fallback={
            <input
              class="pf-input"
              type={field().type === 'integer' ? 'number' : 'text'}
              min={field().min}
              max={field().max}
              placeholder={field().example || ''}
              value={Array.isArray(value()) ? joinList(value()) : (value() ?? '')}
              onChange={(event) => report(event.currentTarget.value)}
            />
          }
        >
          <span class="pf-checks">
            <For each={field().values}>
              {(option) => (
                <label>
                  <input
                    type="checkbox"
                    checked={(value() || []).includes(option)}
                    onChange={(event) => {
                      const current = new Set(value() || []);
                      if (event.currentTarget.checked) current.add(option);
                      else current.delete(option);
                      report([...current]);
                    }}
                  />
                  {option}
                </label>
              )}
            </For>
          </span>
        </Show>
      }
    >
      <select class="pf-input" value={value() ?? ''} onChange={(event) => report(event.currentTarget.value)}>
        <Show when={!field().required}><option value="">(default)</option></Show>
        <For each={field().values}>{(option) => <option value={option}>{option}</option>}</For>
      </select>
    </Show>
  );
}

function ListSection(props) {
  const rows = () => props.spec?.[props.section.key]?.[props.section.list_key] || [];
  const put = (next) =>
    props.onChange({ ...props.spec, [props.section.key]: { ...(props.spec?.[props.section.key] || {}), [props.section.list_key]: next } });

  return (
    <div class="pf-block">
      <table class="pf-table">
        <thead>
          <tr><For each={props.section.fields}>{(field) => <th>{field.name}{field.required ? ' *' : ''}</th>}</For><th /></tr>
        </thead>
        <tbody>
          <For each={rows()}>
            {(row, index) => (
              <tr>
                <For each={props.section.fields}>
                  {(field) => (
                    <td>
                      <FieldInput
                        field={field}
                        value={row[field.name]}
                        onChange={(value) => put(rows().map((item, i) => (i === index() ? withField(item, field.name, value) : item)))}
                      />
                    </td>
                  )}
                </For>
                <td><button type="button" class="tiny" onClick={() => put(rows().filter((_, i) => i !== index()))}>Remove</button></td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <button type="button" class="secondary tiny" onClick={() => put([...rows(), {}])}>Add row</button>
    </div>
  );
}

function ObjectSection(props) {
  const data = () => props.spec?.[props.section.key] || {};
  return (
    <div class="pf-block pf-fields">
      <For each={props.section.fields}>
        {(field) => (
          <label class="pf-field">
            <span>{field.name}</span>
            <FieldInput
              field={field}
              value={data()[field.name]}
              onChange={(value) => props.onChange({ ...props.spec, [props.section.key]: withField(data(), field.name, value) })}
            />
          </label>
        )}
      </For>
    </div>
  );
}

function NetworkSection(props) {
  const groups = () => props.spec?.network_policies || {};
  const put = (next) => props.onChange({ ...props.spec, network_policies: next });
  const endpointFields = () => props.section.endpoint_fields || [];
  const groupFields = () => props.section.fields || [];

  return (
    <div class="pf-block">
      <Show when={Object.keys(groups()).length === 0}>
        <p class="muted small">No network access is granted. A sandbox with an empty network section can reach nothing.</p>
      </Show>
      <For each={Object.entries(groups())}>
        {([name, group]) => (
          <fieldset class="pf-group">
            <legend>
              <input
                class="pf-input pf-name"
                value={name}
                onChange={(event) => put(renameGroup(groups(), name, event.currentTarget.value))}
              />
            </legend>
            <div class="pf-fields">
              <For each={groupFields()}>
                {(field) => (
                  <label class="pf-field">
                    <span>{field.name}</span>
                    <FieldInput
                      field={field}
                      value={group[field.name]}
                      onChange={(value) => put({ ...groups(), [name]: withField(group, field.name, value) })}
                    />
                  </label>
                )}
              </For>
            </div>
            <table class="pf-table">
              <thead><tr><For each={endpointFields()}>{(field) => <th>{field.name}{field.required ? ' *' : ''}</th>}</For><th /></tr></thead>
              <tbody>
                <For each={group.endpoints || []}>
                  {(endpoint, index) => (
                    <tr>
                      <For each={endpointFields()}>
                        {(field) => (
                          <td>
                            <FieldInput
                              field={field}
                              value={endpoint[field.name]}
                              onChange={(value) =>
                                put({
                                  ...groups(),
                                  [name]: {
                                    ...group,
                                    endpoints: group.endpoints.map((item, i) => (i === index() ? withField(item, field.name, value) : item)),
                                  },
                                })
                              }
                            />
                          </td>
                        )}
                      </For>
                      <td>
                        <button type="button" class="tiny"
                          onClick={() => put({ ...groups(), [name]: { ...group, endpoints: group.endpoints.filter((_, i) => i !== index()) } })}>
                          Remove
                        </button>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
            <div class="controls">
              <button type="button" class="secondary tiny"
                onClick={() => put({ ...groups(), [name]: { ...group, endpoints: [...(group.endpoints || []), blankEndpoint()] } })}>
                Add endpoint
              </button>
              <button type="button" class="tiny"
                onClick={() => { const rest = { ...groups() }; delete rest[name]; put(rest); }}>
                Remove group
              </button>
            </div>
          </fieldset>
        )}
      </For>
      <button type="button" class="secondary tiny"
        onClick={() => put({ ...groups(), [freshGroupName(groups())]: { enforcement: 'enforced', endpoints: [blankEndpoint()] } })}>
        Add network group
      </button>
    </div>
  );
}

export default function PolicyForm(props) {
  return (
    <div class="policy-form">
      <For each={props.schema?.sections || []}>
        {(section) => (
          <section class="pf-section">
            <h4>
              {section.title}
              <Show when={!section.hot_reloadable}><span class="muted small"> · needs a recreate to apply</span></Show>
              <Show when={section.hot_reloadable}><span class="muted small"> · applies to running sandboxes</span></Show>
            </h4>
            <p class="muted small">{section.description}</p>
            <Show when={section.list_key}>
              <ListSection section={section} spec={props.spec} onChange={props.onChange} />
            </Show>
            <Show when={section.endpoint_fields}>
              <NetworkSection section={section} spec={props.spec} onChange={props.onChange} />
            </Show>
            <Show when={!section.list_key && !section.endpoint_fields}>
              <ObjectSection section={section} spec={props.spec} onChange={props.onChange} />
            </Show>
          </section>
        )}
      </For>
    </div>
  );
}
