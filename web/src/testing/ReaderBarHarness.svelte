<script lang="ts">
  import { untrack } from 'svelte'
  import ReaderBar from '../lib/ReaderBar.svelte'

  let { count, start = 0 }: { count: number; start?: number } = $props()

  // Only the page it opens on; where it goes after that is the reader's.
  let current = $state(untrack(() => start))
</script>

<output data-testid="current">{current}</output>
<ReaderBar {count} bind:current />
