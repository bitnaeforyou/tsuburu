<script lang="ts">
  import type { Mode, Scope, Sort, SearchState } from './router'
  import { t, type Key } from './i18n.svelte'

  // 검색 실행과 필터는 같은 성격의 도구이므로 한 줄에 모은다. 제출은 텍스트가
  // 아니라 아이콘이다. 위 줄의 Search 탭과 글자가 같으면 무엇이 이동이고
  // 무엇이 실행인지 구분할 수 없다.
  let {
    params,
    onchange,
    localAvailable = false,
    dialogueAvailable = false,
  }: {
    params: SearchState
    onchange: (changes: Partial<SearchState>) => void
    /** Whether a metadata snapshot is loaded; enables the local scope. */
    localAvailable?: boolean
    /** Whether anything has been recognised; enables the dialogue scope. */
    dialogueAvailable?: boolean
  } = $props()

  const SORTS: { value: Sort; key: Key }[] = [
    { value: 'date', key: 'search.sortDate' },
    { value: 'today', key: 'search.sortToday' },
    { value: 'week', key: 'search.sortWeek' },
    { value: 'month', key: 'search.sortMonth' },
    { value: 'year', key: 'search.sortYear' },
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

<search class="toolbar">
  <form onsubmit={submit}>
    <input
      bind:value={input}
      placeholder={params.scope === 'local'
        ? t('search.placeholderLocal')
        : t('search.placeholder')}
      aria-label={t('nav.search')}
      autocomplete="off"
    />
    {#if input}
      <button type="button" class="clear" onclick={clear} aria-label={t('search.clear')}>&times;</button>
    {/if}
    <button type="submit" class="submit" aria-label={t('nav.search')} title={t('nav.search')}>
      <svg viewBox="0 0 20 20" aria-hidden="true" width="16" height="16">
        <circle cx="9" cy="9" r="5.5" fill="none" stroke="currentColor" stroke-width="1.5" />
        <line x1="13" y1="13" x2="17.5" y2="17.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
      </svg>
    </button>
  </form>

  <div class="filters">
    {#if localAvailable || dialogueAvailable}
      <label>
        <span>{t('search.in')}</span>
        <select value={params.scope} onchange={(e) => onchange({ scope: e.currentTarget.value as Scope })}>
          <option value="all">{t('search.scopeAll')}</option>
          <option value="hitomi">{t('search.scopeHitomi')}</option>
          {#if localAvailable}<option value="local">{t('search.scopeLocal')}</option>{/if}
          {#if dialogueAvailable}<option value="dialogue">{t('search.scopeDialogue')}</option>{/if}
        </select>
      </label>
    {/if}

    <!-- Only the dialogue can be asked either way, so only there is it asked. -->
    {#if params.scope === 'dialogue'}
      <label>
        <span>{t('dialogue.mode')}</span>
        <select
          value={params.mode}
          onchange={(e) => onchange({ mode: e.currentTarget.value as Mode })}
        >
          <option value="words">{t('dialogue.modeWords')}</option>
          <option value="meaning">{t('dialogue.modeMeaning')}</option>
        </select>
      </label>
    {/if}
    <!-- The snapshot has no popularity data, so sorting is offered wherever
         hitomi is being asked - which includes asking every source at once,
         where it was already being honoured but could not be reached. -->
    {#if params.scope === 'hitomi' || params.scope === 'all'}
      <label>
        <span>{t('search.sort')}</span>
        <select value={params.sort} onchange={(e) => onchange({ sort: e.currentTarget.value as Sort })}>
          {#each SORTS as option (option.value)}
            <option value={option.value}>{t(option.key)}</option>
          {/each}
        </select>
      </label>
    {/if}
    <label>
      <span>{t('search.language')}</span>
      <select value={params.language} onchange={(e) => onchange({ language: e.currentTarget.value })}>
        {#each LANGUAGES as value (value)}
          <option {value}>{t(`lang.${value}` as Key)}</option>
        {/each}
      </select>
    </label>
    <label>
      <span>{t('search.type')}</span>
      <select value={params.kind} onchange={(e) => onchange({ kind: e.currentTarget.value })}>
        {#each KINDS as value (value)}
          <option {value}>{t(`kind.${value}` as Key)}</option>
        {/each}
      </select>
    </label>
  </div>
</search>

<style>
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem 1.25rem;
    padding: 0.7rem var(--gutter);
    background: var(--bg);
    border-bottom: 1px solid var(--line);
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
    padding-inline-end: 4.2rem;
  }

  .submit {
    position: absolute;
    inset-inline-end: 0.25rem;
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
    inset-inline-end: 2.2rem;
    padding: 0.1rem 0.4rem;
    background: transparent;
    border-color: transparent;
    color: var(--muted);
    font-size: var(--text-lg);
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
    font-size: var(--text-md);
    color: var(--muted);
  }

  /* A filter under 16px makes iOS zoom the whole page the moment it opens,
     and the way back out is a pinch. */
  @media (max-width: 640px) {
    .filters select {
      font-size: var(--text-base);
    }
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
    border: 1px solid var(--edge);
    border-radius: var(--radius);
    padding: 0.25rem 0.4rem;
  }

  /* A phone fits two of the three filters on a line. They used to keep one
     line and slide sideways, but the scrollbar was hidden and nothing else
     said so, which left the third one off the edge of a 320px screen with no
     way to know it was there. One more row is the cheaper price. */
  @media (max-width: 640px) {
    .toolbar {
      padding: 0.6rem var(--gutter);
      gap: 0.6rem;
    }

    form {
      flex: 1 1 100%;
      max-width: none;
    }

    .filters {
      width: 100%;
      gap: 0.5rem 0.75rem;
    }
    select {
      padding: 0.35rem 0.4rem;
    }
  }
</style>
