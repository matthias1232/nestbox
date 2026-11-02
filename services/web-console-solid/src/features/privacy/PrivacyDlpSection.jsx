// The DLP section of the Content privacy page: summary, findings, changes, exceptions.
//
// Stub: F-2 implements it. It exists so the page and the workbox can both point
// at one section instead of at three tables.
export default function PrivacyDlpSection() {
  return (
    <section class="tab-panel">
      <h3>Data loss protection</h3>
      <p class="muted small">Zusammenfassung und Vorkommnisse erscheinen hier.</p>
    </section>
  );
}
