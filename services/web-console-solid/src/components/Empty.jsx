// A consistent empty state, so "nothing here" never looks like "still loading".

export default function Empty(props) {
  return (
    <div class="empty">
      <strong>{props.title}</strong>
      {props.hint && <p class="muted">{props.hint}</p>}
      {props.action}
    </div>
  );
}
