<script lang="ts">
  import { t } from './i18n.svelte'

  let { onconfirm }: { onconfirm: () => void } = $props()

  function confirm() {
    localStorage.setItem('tsuburu.age', 'ok')
    onconfirm()
  }
</script>

<div class="gate">
  <div class="card">
    <h1>tsuburu</h1>
    <p>{t('age.body')}</p>
    <p class="muted">{t('age.privacy')}</p>
    <button onclick={confirm}>{t('age.confirm')}</button>
  </div>
</div>

<style>
  .gate {
    min-height: 100dvh;
    display: grid;
    place-items: center;
    padding: 1.5rem;
  }
  .card {
    max-width: 30rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 1.5rem;
  }
  h1 { margin: 0 0 0.75rem; font-size: 1.4rem; }
  p { margin: 0 0 0.75rem; }
  .muted { color: var(--muted); font-size: 0.9rem; }
  button { margin-top: 0.5rem; }
</style>
