// Usage and cost of every model call, for the whole environment.
import UsageView from './UsageView.jsx';

export default function UsagePage() {
  return (
    <div class="page">
      <h2>Usage &amp; cost</h2>
      <UsageView defaultGroup="model" />
    </div>
  );
}
