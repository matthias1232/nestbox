// The model fit verdict.
//
// The reason comes from the server, not from this component: the server decided
// the fit and it is the only thing that knows the numbers behind the decision.

const LABELS = {
  installable: 'can install',
  cpu_installable: 'runs on CPU',
  already_installed: 'installed',
  update_available: 'update available',
  insufficient_vram: 'not enough memory',
  insufficient_gpus: 'not enough GPUs',
  insufficient_ram: 'not enough RAM',
  insufficient_cpu: 'too few cores',
  needs_unified_memory: 'needs unified memory',
  no_gpu: 'no GPU',
  remote: 'cloud provider',
};

const TONES = {
  installable: 'ok',
  cpu_installable: 'ok',
  already_installed: 'ok',
  update_available: 'warn',
  insufficient_vram: 'bad',
  insufficient_gpus: 'bad',
  insufficient_ram: 'bad',
  insufficient_cpu: 'bad',
  needs_unified_memory: 'info',
  no_gpu: 'muted',
  remote: 'info',
};

export default function FitBadge(props) {
  const fit = () => props.fit?.fit || 'remote';
  return (
    <span class={`badge ${TONES[fit()] || 'muted'}`} title={props.fit?.reason}>
      <i />{LABELS[fit()] || fit()}
    </span>
  );
}
