<script lang="ts">
  import { untrack } from 'svelte'
  import type { Page } from '../lib/api'
  import ReaderView from '../lib/ReaderView.svelte'

  /// Stands in for the route: holds the page the reader is on, so a test can
  /// see where a key or a tap moved it, and can move it from outside.
  let {
    pages,
    start = 0,
    onback,
    onchrome,
  }: { pages: Page[]; start?: number; onback?: () => void; onchrome?: () => void } = $props()

  // Only the page it opens on; where it goes after that is the reader's.
  let current = $state(untrack(() => start))

  export function goto(page: number) {
    current = page
  }
</script>

<output data-testid="current">{current}</output>
<ReaderView {pages} bind:current {onback} {onchrome} />
