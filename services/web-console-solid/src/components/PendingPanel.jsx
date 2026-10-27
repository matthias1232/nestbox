// A panel for a tab whose body is owned by a later package.
//
// It says what will be here, what it will read, and which package fills it. The
// alternatives were an empty box (looks broken) and fake data (looks finished),
// and both are worse than one honest sentence.

export default function PendingPanel(props) {
  return (
    <div class="pending">
      <strong>{props.title}</strong>
      <p class="muted">{props.text}</p>
      <p class="muted small mono">{props.endpoint || ''}</p>
      <p class="muted small">{props.owner}</p>
    </div>
  );
}
