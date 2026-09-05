<script lang="ts">
  let { onconfirm }: { onconfirm: () => void } = $props()

  function confirm() {
    localStorage.setItem('tsuburu.age', 'ok')
    onconfirm()
  }
</script>

<div class="gate">
  <div class="card">
    <h1>tsuburu</h1>
    <p>
      This application searches and displays content from hitomi.la, which includes
      adult material. It is intended for adults only.
    </p>
    <p class="muted">
      tsuburu runs entirely on your own machine. It does not host or redistribute
      anything; it relays only the requests you make.
    </p>
    <button onclick={confirm}>I am 18 or older — continue</button>
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
