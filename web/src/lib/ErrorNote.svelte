<script lang="ts">
  import { ApiError } from './api'
  import { t, type Key } from './i18n.svelte'

  let { error, onretry }: { error: unknown; onretry?: () => void } = $props()

  // 사이트가 바뀐 것과 연결이 안 되는 것은 사용자가 할 일이 다르다.
  const formatChanged = $derived(error instanceof ApiError && error.kind === 'format_changed')
  // The server writes its detail in English. The sentence above it is the
  // reader's own language, so every error says something they can act on.
  const KINDS: Record<string, Key> = {
    network: 'error.network',
    storage: 'error.storage',
    bad_request: 'error.badRequest',
    unsupported: 'error.unsupported',
  }
  const explanation = $derived(
    error instanceof ApiError && !formatChanged && KINDS[error.kind]
      ? t(KINDS[error.kind])
      : null,
  )
  const detail = $derived(error instanceof Error ? error.message : String(error))
</script>

<div class="note" class:stale={formatChanged}>
  <strong>
    {formatChanged ? t('error.changed') : t('error.generic')}
  </strong>
  {#if explanation}
    <p>{explanation}</p>
    <p class="muted">{detail}</p>
  {:else}
    <p>{detail}</p>
  {/if}
  {#if formatChanged}
    <p class="muted">{t('error.changedHint')}</p>
  {:else if onretry}
    <button onclick={onretry}>{t('common.retry')}</button>
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
