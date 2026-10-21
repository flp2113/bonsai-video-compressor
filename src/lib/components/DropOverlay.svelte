<div class="overlay" aria-hidden="true">
  <span>Drop to load</span>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 8px;
    display: grid;
    place-items: center;
    border: 2px solid var(--accent);
    border-radius: 10px;
    background: color-mix(in srgb, var(--bg) 82%, var(--accent));
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--accent);
    pointer-events: none;
    z-index: 10;
  }
</style>
