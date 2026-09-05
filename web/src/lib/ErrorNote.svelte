<script lang="ts">
  import { ApiError } from './api'

  let { error, onretry }: { error: unknown; onretry?: () => void } = $props()

  // 사이트가 바뀐 것과 연결이 안 되는 것은 사용자가 할 일이 다르다.
  const formatChanged = $derived(error instanceof ApiError && error.kind === 'format_changed')
  const message = $derived(error instanceof Error ? error.message : String(error))
</script>

<div class="note" class:stale={formatChanged}>
  <strong>
    {formatChanged ? 'hitomi has changed' : 'Something went wrong'}
  </strong>
  <p>{message}</p>
  {#if formatChanged}
    <p class="muted">Retrying will not help until tsuburu is updated.</p>
  {:else if onretry}
    <button onclick={onretry}>Try again</button>
  {/if}
</div>

<style>
  .note {
    border: 1px solid var(--border);
    border-left: 3px solid var(--danger);
    border-radius: var(--radius);
    background: var(--surface);
    padding: 1rem;
    margin: 1rem 0;
  }
  p { margin: 0.4rem 0; }
  .muted { color: var(--muted); font-size: 0.9rem; }
</style>
