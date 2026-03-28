<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { Minus, Square, Copy, X } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button";
  import { activeConnection } from "$lib/stores/connections";

  const appWindow = getCurrentWindow();

  let isMaximized = $state(false);

  onMount((): (() => void) => {
    let unlisten: (() => void) | undefined;

    (async () => {
      isMaximized = await appWindow.isMaximized();
      unlisten = await appWindow.onResized(async () => {
        isMaximized = await appWindow.isMaximized();
      });
    })();

    return () => unlisten?.();
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
  let pendingDragTimer: ReturnType<typeof setTimeout> | undefined;

  function startDrag(e: MouseEvent) {
    if (e.button !== 0 || (e.target as Element).closest("button")) return;

    const now = Date.now();

    if (now - lastMouseDownTime < DOUBLE_CLICK_MS) {
      lastMouseDownTime = 0;
      clearTimeout(pendingDragTimer);
      maximize();
      return;
    }

    lastMouseDownTime = now;

    const cleanup = () => {
      clearTimeout(pendingDragTimer);
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

    pendingDragTimer = setTimeout(() => {
      cleanup();
      appWindow.startDragging();
    }, 150);
  }
</script>

<div
  class="h-10 flex items-center justify-between bg-background border-b border-border select-none"
  role="toolbar"
  tabindex="-1"
  onmousedown={startDrag}
>
  <!-- Title area (draggable) -->
  <div class="flex-1 flex items-center gap-2 px-4">
    <span class="text-sm font-semibold text-foreground">GSDB</span>
    <span class="text-xs text-muted-foreground">Database Management Tool</span>
    {#if $activeConnection}
      <span class="text-xs text-muted-foreground italic"
        >({$activeConnection.database})</span
      >
    {/if}
  </div>

  <!-- Window controls -->
  <div class="flex items-center">
    <Button
      variant="ghost"
      size="icon"
      class="h-10 w-10 rounded-none hover:bg-accent"
      onclick={minimize}
    >
      <Minus class="w-4 h-4" />
    </Button>
    <Button
      variant="ghost"
      size="icon"
      class="h-10 w-10 rounded-none hover:bg-accent"
      onclick={maximize}
    >
      {#if isMaximized}
        <Copy class="w-4 h-4" />
      {:else}
        <Square class="w-4 h-4" />
      {/if}
    </Button>
    <Button
      variant="ghost"
      size="icon"
      class="h-10 w-10 rounded-none hover:bg-destructive hover:text-destructive-foreground"
      onclick={close}
    >
      <X class="w-4 h-4" />
    </Button>
  </div>
</div>
