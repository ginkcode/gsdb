<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { Database } from "@lucide/svelte";
  import { activeConnection } from "$lib/stores/connections";
  import { platform } from "$lib/stores/platform";

  const appWindow = getCurrentWindow();

  let isMaximized = $state(false);
  let isFocused = $state(true);

  const os = $derived($platform);
  const isMac = $derived(os === "macos");
  const isWindows = $derived(os === "windows");

  // Connection names often already embed the database (e.g. "postgres/dtp"),
  // so only append it when it actually adds information.
  const subtitle = $derived.by(() => {
    const conn = $activeConnection;
    if (!conn) return null;
    const db = conn.database?.trim();
    return db && !conn.name.includes(db) ? `${conn.name} · ${db}` : conn.name;
  });

  onMount((): (() => void) => {
    const unlisteners: Array<() => void> = [];
    let disposed = false;

    (async () => {
      platform.initialize();
      isMaximized = await appWindow.isMaximized();
      isFocused = await appWindow.isFocused();

      const stop = [
        await appWindow.onResized(async () => {
          isMaximized = await appWindow.isMaximized();
        }),
        await appWindow.onFocusChanged(({ payload }) => {
          isFocused = payload;
        }),
      ];

      if (disposed) stop.forEach((fn) => fn());
      else unlisteners.push(...stop);
    })();

    return () => {
      disposed = true;
      unlisteners.forEach((fn) => fn());
    };
  });

  async function minimize() {
    await appWindow.minimize();
  }

  async function maximize() {
    if (isMaximized) {
      await appWindow.unmaximize();
    } else {
      await appWindow.maximize();
    }
  }

  async function close() {
    await appWindow.close();
  }

  const DOUBLE_CLICK_MS = 300;
  let lastMouseDownTime = 0;

  function startDrag(e: MouseEvent) {
    if (e.button !== 0 || (e.target as Element).closest("button")) return;

    const now = Date.now();

    if (now - lastMouseDownTime < DOUBLE_CLICK_MS) {
      lastMouseDownTime = 0;
      maximize();
      return;
    }

    lastMouseDownTime = now;

    const cleanup = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };

    const onMove = () => {
      cleanup();
      appWindow.startDragging();
    };

    const onUp = () => cleanup();

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  // Caption glyphs are drawn as hairline SVG rather than icon-font glyphs so
  // they match the 1px stroke the OS shell uses at this size.
  const CAPTION_BTN =
    "inline-flex items-center justify-center outline-none transition-colors " +
    "focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-inset";

  // Windows: full-height 46px cells, flush to the corner.
  const WIN_BTN = `${CAPTION_BTN} h-full w-[46px] text-foreground/80 hover:bg-foreground/10 active:bg-foreground/[0.06]`;
  const WIN_CLOSE = `${CAPTION_BTN} h-full w-[46px] text-foreground/80 hover:bg-[#c42b1c] hover:text-white active:bg-[#c42b1c]/85`;

  // macOS traffic lights carry a faint dark rim; without it they read as flat dots.
  const TRAFFIC =
    "h-3 w-3 rounded-full flex items-center justify-center outline-none " +
    "shadow-[inset_0_0_0_0.5px_rgba(0,0,0,0.22)]";

  // Linux (Adwaita/Breeze): 24px pills with a resting surface.
  const NIX_BTN = `${CAPTION_BTN} h-6 w-6 rounded-full bg-foreground/[0.08] text-foreground/75 hover:bg-foreground/20 hover:text-foreground active:bg-foreground/25`;
  const NIX_CLOSE = `${CAPTION_BTN} h-6 w-6 rounded-full bg-foreground/[0.08] text-foreground/75 hover:bg-[#c42b1c] hover:text-white active:bg-[#c42b1c]/85`;
</script>

{#snippet glyphMinimize()}
  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
    <path
      d="M0 5.5h10"
      stroke="currentColor"
      stroke-width="1"
      shape-rendering="crispEdges"
    />
  </svg>
{/snippet}

{#snippet glyphMaximize()}
  <svg width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
    <rect
      x="0.5"
      y="0.5"
      width="9"
      height="9"
      rx="1.5"
      stroke="currentColor"
      stroke-width="1"
    />
  </svg>
{/snippet}

{#snippet glyphRestore()}
  <svg width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
    <path
      d="M2.7 2.7V1.9A1.4 1.4 0 0 1 4.1.5H8.1A1.4 1.4 0 0 1 9.5 1.9V5.9A1.4 1.4 0 0 1 8.1 7.3H7.3"
      stroke="currentColor"
      stroke-width="1"
    />
    <rect
      x="0.5"
      y="2.7"
      width="6.8"
      height="6.8"
      rx="1.4"
      stroke="currentColor"
      stroke-width="1"
    />
  </svg>
{/snippet}

{#snippet glyphClose()}
  <svg width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
    <path d="M0.7 0.7l8.6 8.6M9.3 0.7L0.7 9.3" stroke="currentColor" stroke-width="1" />
  </svg>
{/snippet}

<!-- Traffic-light glyphs: only revealed while the cluster is hovered, as on macOS -->
{#snippet macGlyph(d: string)}
  <svg
    width="6"
    height="6"
    viewBox="0 0 6 6"
    fill="none"
    class="opacity-0 transition-opacity group-hover/traffic:opacity-100"
    aria-hidden="true"
  >
    <path
      {d}
      stroke="#000"
      stroke-opacity="0.6"
      stroke-width="1.1"
      stroke-linecap="round"
    />
  </svg>
{/snippet}

<!-- Zoom is the one traffic light drawn with filled triangles, not a stroke -->
{#snippet macZoomGlyph()}
  <svg
    width="6"
    height="6"
    viewBox="0 0 6 6"
    class="opacity-0 transition-opacity group-hover/traffic:opacity-100"
    aria-hidden="true"
  >
    <path
      d="M1 1h2.6L1 3.6z M5 5H2.4L5 2.4z"
      fill="#000"
      fill-opacity="0.6"
    />
  </svg>
{/snippet}

<!-- macOS title bars are 28px; Windows/Linux run taller -->
<header
  class="relative shrink-0 flex items-center bg-background border-b select-none {isMac
    ? 'h-7 border-black/20 dark:border-white/[0.07]'
    : 'h-10 border-border'} {isFocused ? '' : 'opacity-90'}"
  role="toolbar"
  tabindex="-1"
  onmousedown={startDrag}
>
  {#if isMac}
    <!-- Traffic lights, leading edge -->
    <div class="group/traffic flex items-center gap-2 pl-[13px] pr-3">
      <button
        type="button"
        class="{TRAFFIC} {isFocused ? 'bg-[#ff5f57]' : 'bg-foreground/20'}"
        aria-label="Close window"
        onclick={close}
      >
        {@render macGlyph("M1.4 1.4l3.2 3.2M4.6 1.4L1.4 4.6")}
      </button>
      <button
        type="button"
        class="{TRAFFIC} {isFocused ? 'bg-[#febc2e]' : 'bg-foreground/20'}"
        aria-label="Minimize window"
        onclick={minimize}
      >
        {@render macGlyph("M1.2 3h3.6")}
      </button>
      <button
        type="button"
        class="{TRAFFIC} {isFocused ? 'bg-[#28c840]' : 'bg-foreground/20'}"
        aria-label={isMaximized ? "Restore window" : "Maximize window"}
        onclick={maximize}
      >
        {@render macZoomGlyph()}
      </button>
    </div>
  {:else if isWindows}
    <!-- Windows puts the app icon and title on the leading edge -->
    <Database class="w-4 h-4 shrink-0 ml-3 text-primary" />
  {/if}

  <!-- Title: centered on macOS/Linux, leading on Windows. Non-interactive so
       the whole strip stays draggable. -->
  <div
    class="absolute inset-y-0 flex items-center pointer-events-none {isWindows
      ? 'left-[38px] right-[150px] justify-start'
      : 'left-[120px] right-[120px] justify-center'}"
  >
    <div
      class="flex items-baseline gap-1.5 min-w-0 text-[13px] {isFocused
        ? ''
        : 'opacity-60'}"
    >
      <span class="font-semibold text-foreground shrink-0">GSDB</span>
      {#if subtitle}
        <span class="text-muted-foreground/60 shrink-0">—</span>
        <span class="text-muted-foreground truncate">{subtitle}</span>
      {/if}
    </div>
  </div>

  <div class="flex-1"></div>

  {#if !isMac}
    <div
      class="flex items-center h-full {isWindows ? '' : 'gap-2 pr-2.5'}"
    >
      <button
        type="button"
        class={isWindows ? WIN_BTN : NIX_BTN}
        aria-label="Minimize window"
        title="Minimize"
        onclick={minimize}
      >
        {@render glyphMinimize()}
      </button>
      <button
        type="button"
        class={isWindows ? WIN_BTN : NIX_BTN}
        aria-label={isMaximized ? "Restore window" : "Maximize window"}
        title={isMaximized ? "Restore" : "Maximize"}
        onclick={maximize}
      >
        {#if isMaximized}
          {@render glyphRestore()}
        {:else}
          {@render glyphMaximize()}
        {/if}
      </button>
      <button
        type="button"
        class={isWindows ? WIN_CLOSE : NIX_CLOSE}
        aria-label="Close window"
        title="Close"
        onclick={close}
      >
        {@render glyphClose()}
      </button>
    </div>
  {/if}
</header>
