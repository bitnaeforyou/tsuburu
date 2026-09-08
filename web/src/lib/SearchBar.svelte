<script lang="ts">
  import type { Scope, Sort, SearchState } from './router'

  // 검색 실행과 필터는 같은 성격의 도구이므로 한 줄에 모은다. 제출은 텍스트가
  // 아니라 아이콘이다. 위 줄의 Search 탭과 글자가 같으면 무엇이 이동이고
  // 무엇이 실행인지 구분할 수 없다.
  let {
    params,
    onchange,
    localAvailable = false,
  }: {
    params: SearchState
    onchange: (changes: Partial<SearchState>) => void
    /** Whether a metadata snapshot is loaded; enables the local scope. */
    localAvailable?: boolean
  } = $props()

  const SORTS: { value: Sort; label: string }[] = [
    { value: 'date', label: 'Newest' },
    { value: 'today', label: 'Popular today' },
    { value: 'week', label: 'Popular this week' },
    { value: 'month', label: 'Popular this month' },
    { value: 'year', label: 'Popular this year' },
  ]
  const LANGUAGES = ['all', 'korean', 'japanese', 'english', 'chinese', 'spanish']
  const KINDS = ['all', 'doujinshi', 'manga', 'artistcg', 'gamecg', 'imageset']

  let input = $state(params.query)

  $effect(() => {
    input = params.query
  })

  function submit(event: SubmitEvent) {
    event.preventDefault()
    onchange({ query: input.trim() })
  }

  function clear() {
    input = ''
    onchange({ query: '' })
  }
</script>

<div class="toolbar">
  <form onsubmit={submit}>
    <input
      bind:value={input}
      placeholder={params.scope === 'local'
        ? 'Title, artist, series or character, in Korean or English'
        : 'Search in Korean or English, use -term to exclude'}
      aria-label="Search"
      autocomplete="off"
    />
    {#if input}
      <button type="button" class="clear" onclick={clear} aria-label="Clear search">&times;</button>
    {/if}
    <button type="submit" class="submit" aria-label="Search" title="Search">
      <svg viewBox="0 0 20 20" aria-hidden="true" width="16" height="16">
        <circle cx="9" cy="9" r="5.5" fill="none" stroke="currentColor" stroke-width="1.8" />
        <line x1="13" y1="13" x2="17.5" y2="17.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
      </svg>
    </button>
  </form>

  <div class="filters">
    {#if localAvailable}
      <label>
        <span>In</span>
        <select value={params.scope} onchange={(e) => onchange({ scope: e.currentTarget.value as Scope })}>
          <option value="hitomi">hitomi tags</option>
          <option value="local">local titles & artists</option>
        </select>
      </label>
    {/if}
    <!-- The snapshot has no popularity data, so sorting only applies to hitomi. -->
    {#if params.scope !== 'local'}
      <label>
        <span>Sort</span>
        <select value={params.sort} onchange={(e) => onchange({ sort: e.currentTarget.value as Sort })}>
          {#each SORTS as option (option.value)}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>
    {/if}
    <label>
      <span>Language</span>
      <select value={params.language} onchange={(e) => onchange({ language: e.currentTarget.value })}>
        {#each LANGUAGES as value (value)}
          <option {value}>{value}</option>
        {/each}
      </select>
    </label>
    <label>
      <span>Type</span>
      <select value={params.kind} onchange={(e) => onchange({ kind: e.currentTarget.value })}>
        {#each KINDS as value (value)}
          <option {value}>{value}</option>
        {/each}
      </select>
    </label>
  </div>
</div>

<style>
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem 1.25rem;
    padding: 0.7rem 1rem;
    background: var(--bg);
    border-bottom: 1px solid var(--border);
  }

  form {
    position: relative;
    display: flex;
    align-items: center;
    flex: 1 1 22rem;
    max-width: 40rem;
  }

  input {
    width: 100%;
    padding-right: 4.2rem;
  }

  .submit {
    position: absolute;
    right: 0.25rem;
    display: grid;
    place-items: center;
    padding: 0.3rem 0.45rem;
    background: transparent;
    border-color: transparent;
    color: var(--muted);
  }
  .submit:hover {
    color: var(--accent);
    border-color: transparent;
  }

  .clear {
    position: absolute;
    right: 2.2rem;
    padding: 0.1rem 0.4rem;
    background: transparent;
    border-color: transparent;
    color: var(--muted);
    font-size: 1.1rem;
    line-height: 1;
  }
  .clear:hover {
    color: var(--text);
    border-color: transparent;
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 0.9rem;
    font-size: 0.85rem;
    color: var(--muted);
  }

  .filters label {
    display: flex;
    align-items: center;
    gap: 0.35rem;
  }

  select {
    font: inherit;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0.25rem 0.4rem;
  }
</style>
