<script lang="ts">
  import { copy } from './clipboard'
  import { t } from './i18n.svelte'

  /// A work's number, copied when pressed: it is what a reader pastes into
  /// hitomi, into a search box, or to someone else.
  let { id, label }: { id: number; label: string } = $props()

  /// What the last press came to, said for a moment in place of the label.
  let said = $state<string | null>(null)
  let timer: ReturnType<typeof setTimeout> | undefined

  async function press() {
    said = (await copy(String(id))) ? t('copy.done') : t('copy.failed')
    clearTimeout(timer)
    timer = setTimeout(() => (said = null), 1500)
  }

  $effect(() => () => clearTimeout(timer))
</script>

<!-- Both labels are always laid out, in one cell, and only one is seen: a
     button that grew to say "copied" pushed its neighbours onto the next
     line and back again a second later. -->
<button class="copy" onclick={press} title={t('copy.idTitle', { id })}>
  <span class:unseen={said !== null}>{label}</span>
  <span class:unseen={said === null} aria-hidden="true">{said ?? t('copy.done')}</span>
</button>
<span class="sr-only" role="status">{said ?? ''}</span>

<style>
  .copy {
    display: inline-grid;
    font-variant-numeric: tabular-nums;
  }
  .copy > span {
    grid-area: 1 / 1;
  }
  .unseen {
    visibility: hidden;
  }
</style>
