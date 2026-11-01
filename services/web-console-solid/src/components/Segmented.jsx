// A row of mutually exclusive buttons.
//
// It is a radio group, not a set of toggle buttons: exactly one option carries the
// active look, so "which mode am I in" is answered by the colour alone.

import { For } from 'solid-js';

import '../styles/controls.css';

export default function Segmented(props) {
  return (
    <div class="nb-segmented" role="radiogroup">
      <For each={props.options || []}>
        {(option) => {
          const active = () => props.value === option.value;
          return (
            <button
              type="button"
              role="radio"
              aria-checked={active()}
              title={option.title}
              class={'nb-seg' + (active() ? ' active' : '') + (option.tone ? ' tone-' + option.tone : '')}
              disabled={props.disabled}
              onClick={() => props.onChange?.(option.value)}
            >
              {option.label}
            </button>
          );
        }}
      </For>
    </div>
  );
}
