<script lang="ts">
  import { theme, type Theme } from "$lib/stores/theme";
  import Icon from "@lucide/svelte/icons/sun";
  import IconMoon from "@lucide/svelte/icons/moon";
  import IconMonitor from "@lucide/svelte/icons/monitor";
  import { onMount } from "svelte";

  const themes: { value: Theme; label: string; icon: typeof Icon }[] = [
    { value: "light", label: "Light", icon: Icon },
    { value: "dark", label: "Dark", icon: IconMoon },
    { value: "system", label: "System", icon: IconMonitor },
  ];

  let open = $state(false);
  let containerEl: HTMLDivElement;

  onMount(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (containerEl && !containerEl.contains(e.target as Node)) {
        open = false;
      }
    };

    document.addEventListener("click", handleClickOutside);
    return () => {
      document.removeEventListener("click", handleClickOutside);
    };
  });

  function setTheme(newTheme: Theme) {
    theme.set(newTheme);
    open = false;
  }
</script>

<div class="relative" bind:this={containerEl}>
  <button
    type="button"
    class="rounded-md p-2 hover:bg-accent transition-colors"
    onclick={() => (open = !open)}
    aria-label="Toggle theme"
  >
    {#if $theme === "light"}
      <Icon class="size-5" />
    {:else if $theme === "dark"}
      <IconMoon class="size-5" />
    {:else}
      <IconMonitor class="size-5" />
    {/if}
  </button>

  {#if open}
    <div
      class="absolute right-0 top-full mt-1 z-50 min-w-30 rounded-md border bg-popover p-1 shadow-md"
    >
      {#each themes as t}
        <button
          type="button"
          class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-sm hover:bg-accent transition-colors {$theme ===
          t.value
            ? 'bg-accent'
            : ''}"
          onclick={() => setTheme(t.value)}
        >
          <t.icon class="size-4" />
          {t.label}
        </button>
      {/each}
    </div>
  {/if}
</div>
